//! A raw terminal socket that records every frame, so nothing a daemon sends is hidden.

use std::time::{Duration, Instant};

use async_tungstenite::WebSocketStream;
use async_tungstenite::tokio::{ConnectStream, connect_async};
use async_tungstenite::tungstenite::Message;
use futures::StreamExt;
use serde_json::Value;

use crate::daemon::Daemon;

const WAIT: Duration = Duration::from_secs(20);

#[derive(Clone, Debug, PartialEq)]
pub enum Got {
    Control(Value),
    Output(Vec<u8>),
}

pub struct Socket {
    socket: Option<WebSocketStream<ConnectStream>>,
    pub frames: Vec<Got>,
    /// Once closed: the close code, 0 for a close frame with no code, None for a dropped
    /// connection.
    pub closed: Option<Option<u16>>,
}

impl Socket {
    /// A socket to `terminal` of the task with a fresh ticket.
    pub async fn open(daemon: &Daemon, query: &str) -> Socket {
        let ticket = daemon.ticket().await;
        Socket::open_raw(daemon, &format!("ticket={ticket}&{query}")).await
    }

    pub async fn open_raw(daemon: &Daemon, query: &str) -> Socket {
        let url = daemon.socket_url(query);
        let socket = connect_async(url.as_str())
            .await
            .ok()
            .map(|(socket, _)| socket);
        let closed = socket.is_none().then_some(None);
        Socket {
            socket,
            frames: Vec::new(),
            closed,
        }
    }

    pub async fn send(&mut self, json: &str) {
        if let Some(socket) = &mut self.socket {
            let _ = socket.send(Message::Text(json.to_string().into())).await;
        }
    }

    pub async fn send_binary(&mut self, bytes: &[u8]) {
        if let Some(socket) = &mut self.socket {
            let _ = socket.send(Message::Binary(bytes.to_vec().into())).await;
        }
    }

    pub async fn input(&mut self, text: &str) {
        let json = serde_json::json!({ "type": "input", "data": text }).to_string();
        self.send(&json).await;
    }

    /// Reads frames until `done` holds or the socket closes, at most WAIT. False on a timeout.
    pub async fn until(&mut self, done: impl Fn(&Socket) -> bool) -> bool {
        let deadline = Instant::now() + WAIT;
        while !done(self) && self.closed.is_none() {
            if !self.read_one(deadline).await {
                return false;
            }
        }
        done(self)
    }

    /// Reads for `time`, whatever comes.
    pub async fn read_for(&mut self, time: Duration) {
        let deadline = Instant::now() + time;
        while self.closed.is_none() && self.read_one(deadline).await {}
    }

    async fn read_one(&mut self, deadline: Instant) -> bool {
        let Some(socket) = &mut self.socket else {
            return false;
        };
        let left = deadline.saturating_duration_since(Instant::now());
        match tokio::time::timeout(left, socket.next()).await {
            Err(_) => false,
            Ok(None | Some(Err(_))) => {
                self.closed = Some(self.closed.flatten());
                true
            }
            Ok(Some(Ok(message))) => {
                self.take(message);
                true
            }
        }
    }

    fn take(&mut self, message: Message) {
        match message {
            Message::Text(text) => {
                let value = serde_json::from_str(text.as_str()).unwrap_or(Value::Null);
                self.frames.push(Got::Control(value));
            }
            Message::Binary(bytes) => self.frames.push(Got::Output(bytes.to_vec())),
            Message::Close(frame) => {
                self.closed = Some(Some(frame.map_or(0, |frame| u16::from(frame.code))));
            }
            _ => {}
        }
    }

    /// Reads until the socket closes. The close code, as `closed`.
    pub async fn close_code(&mut self) -> Option<u16> {
        let deadline = Instant::now() + WAIT;
        while self.closed.is_none() && self.read_one(deadline).await {}
        self.closed.flatten()
    }

    pub fn drop_connection(&mut self) {
        self.socket = None;
    }

    pub fn controls(&self) -> Vec<Value> {
        self.frames
            .iter()
            .filter_map(|got| match got {
                Got::Control(value) => Some(value.clone()),
                Got::Output(_) => None,
            })
            .collect()
    }

    pub fn types(&self) -> Vec<String> {
        self.controls()
            .iter()
            .map(|value| value["type"].as_str().unwrap_or("").to_string())
            .collect()
    }

    pub fn count(&self, kind: &str) -> usize {
        self.types().iter().filter(|known| *known == kind).count()
    }

    /// All the output bytes, in order.
    pub fn bytes(&self) -> Vec<u8> {
        self.frames
            .iter()
            .filter_map(|got| match got {
                Got::Output(bytes) => Some(bytes.as_slice()),
                Got::Control(_) => None,
            })
            .flatten()
            .copied()
            .collect()
    }

    /// The bytes of the last snapshot (between its `snapshot` and the next control message),
    /// with the size it names.
    pub fn last_snapshot(&self) -> Option<(u16, u16, Vec<u8>)> {
        let start = self
            .frames
            .iter()
            .rposition(|got| matches!(got, Got::Control(value) if value["type"] == "snapshot"))?;
        let Got::Control(message) = &self.frames[start] else {
            return None;
        };
        let size = |name: &str| {
            message[name]
                .as_u64()
                .and_then(|value| u16::try_from(value).ok())
        };
        let bytes = self.frames[start + 1..]
            .iter()
            .map_while(|got| match got {
                Got::Output(bytes) => Some(bytes.as_slice()),
                Got::Control(_) => None,
            })
            .flatten()
            .copied()
            .collect();
        Some((size("cols")?, size("rows")?, bytes))
    }

    /// The screen a dashboard of this size shows after all the output, as the client draws it:
    /// at the size of the last snapshot, then fitted to its own.
    pub fn screen(&self, cols: u16, rows: u16) -> Vec<String> {
        let (first_cols, first_rows) = self
            .last_snapshot()
            .map_or((cols, rows), |(cols, rows, _)| (cols, rows));
        let mut screen = natamux_screen::Screen::new(first_cols, first_rows);
        // The dashboard resets before a snapshot: what came before it is gone.
        let start = self
            .frames
            .iter()
            .rposition(|got| matches!(got, Got::Control(value) if value["type"] == "snapshot"))
            .unwrap_or(0);
        for got in &self.frames[start..] {
            if let Got::Output(bytes) = got {
                screen.write(bytes);
            }
        }
        screen.resize(cols, rows);
        screen
            .last_lines(usize::MAX)
            .split('\n')
            .map(String::from)
            .collect()
    }

    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.bytes()).into_owned()
    }
}
