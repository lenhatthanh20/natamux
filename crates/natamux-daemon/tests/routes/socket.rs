//! The WebSocket client of the route tests: what a socket got, and ways to wait for it.

use std::time::{Duration, Instant};

use async_tungstenite::WebSocketStream;
use async_tungstenite::tokio::ConnectStream;
use async_tungstenite::tungstenite::Message;
use futures::StreamExt;
use serde_json::Value;

/// What a client got: text frames as JSON, binary ones as text.
#[derive(Debug, Clone, PartialEq)]
pub enum Got {
    Control(Value),
    Output(String),
}

pub struct Socket {
    socket: WebSocketStream<ConnectStream>,
    pub frames: Vec<Got>,
    /// The close code, once closed (0: closed with no code).
    pub closed: Option<u16>,
}

impl Socket {
    pub fn new(socket: WebSocketStream<ConnectStream>) -> Socket {
        Socket {
            socket,
            frames: Vec::new(),
            closed: None,
        }
    }

    pub async fn send(&mut self, json: &str) {
        self.socket
            .send(Message::Text(json.to_string().into()))
            .await
            .unwrap();
    }

    /// A protocol ping frame, as a browser never sends but a proxy may.
    pub async fn send_ping(&mut self) {
        self.socket
            .send(Message::Ping(Vec::new().into()))
            .await
            .unwrap();
    }

    /// Reads for `ms`; true when the socket is still open after it.
    pub async fn open_after(&mut self, ms: u64) -> bool {
        let deadline = Instant::now() + Duration::from_millis(ms);
        while self.closed.is_none() {
            let left = deadline.saturating_duration_since(Instant::now());
            match tokio::time::timeout(left, self.socket.next()).await {
                Err(_) => break,
                Ok(None | Some(Err(_))) => self.closed = Some(0),
                Ok(Some(Ok(message))) => self.take(message),
            }
        }
        self.closed.is_none()
    }

    pub async fn send_binary(&mut self, bytes: &[u8]) {
        self.socket
            .send(Message::Binary(bytes.to_vec().into()))
            .await
            .unwrap();
    }

    /// Reads frames until `done` holds, at most 5 s.
    pub async fn until(&mut self, done: impl Fn(&Socket) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !done(self) && self.closed.is_none() {
            let left = deadline.saturating_duration_since(Instant::now());
            match tokio::time::timeout(left, self.socket.next()).await {
                Err(_) => panic!("timed out; got {:?}", self.frames),
                Ok(None | Some(Err(_))) => self.closed = Some(self.closed.unwrap_or(0)),
                Ok(Some(Ok(message))) => self.take(message),
            }
        }
        assert!(done(self), "closed early; got {:?}", self.frames);
    }

    fn take(&mut self, message: Message) {
        match message {
            Message::Text(text) => {
                self.frames
                    .push(Got::Control(serde_json::from_str(text.as_str()).unwrap()));
            }
            Message::Binary(bytes) => {
                self.frames
                    .push(Got::Output(String::from_utf8_lossy(&bytes).into_owned()));
            }
            Message::Close(frame) => {
                self.closed = Some(frame.map_or(0, |frame| u16::from(frame.code)));
            }
            _ => {}
        }
    }

    /// Reads until the socket closes; gives the close code (0: no code).
    pub async fn close_code(&mut self) -> u16 {
        let deadline = Instant::now() + Duration::from_secs(5);
        while self.closed.is_none() {
            let left = deadline.saturating_duration_since(Instant::now());
            match tokio::time::timeout(left, self.socket.next()).await {
                Err(_) => panic!("not closed; got {:?}", self.frames),
                Ok(None | Some(Err(_))) => self.closed = Some(0),
                Ok(Some(Ok(message))) => self.take(message),
            }
        }
        self.closed.unwrap_or(0)
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

    pub fn replayed(&self) -> bool {
        self.types().iter().any(|kind| kind == "replayed")
    }

    /// The live output: the binary frames after the first `replayed`.
    pub fn output(&self) -> String {
        let start = self
            .frames
            .iter()
            .position(|got| matches!(got, Got::Control(value) if value["type"] == "replayed"))
            .map_or(self.frames.len(), |at| at + 1);
        self.frames[start..]
            .iter()
            .filter_map(|got| match got {
                Got::Output(text) => Some(text.as_str()),
                Got::Control(_) => None,
            })
            .collect()
    }

    pub async fn drop_connection(self) {
        drop(self.socket);
    }
}

/// Waits up to 5 s for `done`.
pub async fn eventually(done: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !done() {
        assert!(Instant::now() < deadline, "condition not met in time");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}
