//! An in-process fake daemon for the client's tests: plain HTTP for the routes, and WebSockets
//! whose daemon side each test scripts by hand.

// Each test file uses a part of it.
#![allow(dead_code)]

use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_tungstenite::WebSocketStream;
use async_tungstenite::tokio::{TokioAdapter, accept_async};
use async_tungstenite::tungstenite::Message;
use async_tungstenite::tungstenite::protocol::CloseFrame;
use async_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use futures::StreamExt;
use natamux_client::{Client, Config};
use natamux_proto::terminals::messages::ClientMessage;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;

pub const TOKEN: &str = "secret";
pub const WAIT: Duration = Duration::from_secs(5);

pub type Socket = WebSocketStream<TokioAdapter<TcpStream>>;

/// One WebSocket the client opened, with the path and query it asked for.
pub struct Conn {
    pub target: String,
    pub socket: Socket,
}

impl Conn {
    pub async fn text(&mut self, json: &str) {
        self.socket
            .send(Message::text(json.to_string()))
            .await
            .unwrap();
    }

    pub async fn binary(&mut self, bytes: &[u8]) {
        self.socket
            .send(Message::binary(bytes.to_vec()))
            .await
            .unwrap();
    }

    pub async fn close(&mut self, code: u16) {
        let frame = CloseFrame {
            code: CloseCode::from(code),
            reason: String::new().into(),
        };
        let _ = self.socket.close(Some(frame)).await;
    }

    /// The next JSON message from the client; pings are skipped unless asked for.
    pub async fn recv(&mut self, keep_pings: bool) -> ClientMessage {
        loop {
            let frame = within(self.socket.next()).await.unwrap().unwrap();
            if let Message::Text(text) = frame {
                let message: ClientMessage = serde_json::from_str(text.as_str()).unwrap();
                if keep_pings || message != ClientMessage::Ping {
                    return message;
                }
            }
        }
    }
}

#[derive(Default)]
pub struct State {
    pub tasks: String,
    pub tickets: u32,
    pub task_fetches: u32,
    pub closed_terminals: Vec<String>,
}

pub struct FakeDaemon {
    pub address: String,
    state: Arc<Mutex<State>>,
    terminals: mpsc::UnboundedReceiver<Conn>,
    events: mpsc::UnboundedReceiver<Conn>,
}

impl FakeDaemon {
    pub async fn start() -> FakeDaemon {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let state = Arc::new(Mutex::new(State {
            tasks: tasks_json(&["T-1"]),
            ..State::default()
        }));
        let (terminal_tx, terminals) = mpsc::unbounded_channel();
        let (events_tx, events) = mpsc::unbounded_channel();
        let shared = state.clone();
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let (state, terminal_tx, events_tx) =
                    (shared.clone(), terminal_tx.clone(), events_tx.clone());
                tokio::spawn(serve(stream, state, terminal_tx, events_tx));
            }
        });
        FakeDaemon {
            address,
            state,
            terminals,
            events,
        }
    }

    pub fn client(&self) -> Client {
        Client::new(Config {
            base_url: format!("http://{}", self.address),
            token: TOKEN.into(),
        })
        .unwrap()
    }

    pub async fn next_terminal(&mut self) -> Conn {
        within(self.terminals.recv()).await.unwrap()
    }

    pub async fn next_events(&mut self) -> Conn {
        within(self.events.recv()).await.unwrap()
    }

    pub fn state<T>(&self, read: impl FnOnce(&mut State) -> T) -> T {
        read(&mut self.state.lock().unwrap())
    }
}

pub async fn serve(
    mut stream: TcpStream,
    state: Arc<Mutex<State>>,
    terminals: mpsc::UnboundedSender<Conn>,
    events: mpsc::UnboundedSender<Conn>,
) {
    let head = peek_head(&stream).await;
    let first = head.lines().next().unwrap_or_default().to_string();
    let mut parts = first.split(' ');
    let (method, target) = (
        parts.next().unwrap_or_default().to_string(),
        parts.next().unwrap_or_default().to_string(),
    );
    let lower = head.to_lowercase();
    if lower.contains("upgrade: websocket") {
        if target.starts_with("/events") {
            if !target.contains(&format!("token={TOKEN}")) {
                let _ = stream
                    .write_all(b"HTTP/1.1 401 Unauthorized\r\ncontent-length: 0\r\n\r\n")
                    .await;
                return;
            }
            let socket = accept_async(stream).await.unwrap();
            let _ = events.send(Conn { target, socket });
        } else {
            let socket = accept_async(stream).await.unwrap();
            let _ = terminals.send(Conn { target, socket });
        }
        return;
    }
    let mut buffer = vec![0; head.len()];
    stream.read_exact(&mut buffer).await.unwrap();
    let authorized = lower.contains(&format!("authorization: bearer {TOKEN}"));
    let (status, body) = if !authorized {
        (
            "401 Unauthorized",
            r#"{"error":"unauthorized"}"#.to_string(),
        )
    } else if method == "GET" && target == "/tasks" {
        let mut state = state.lock().unwrap();
        state.task_fetches += 1;
        ("200 OK", state.tasks.clone())
    } else if method == "POST" && target.ends_with("/terminal/ticket") {
        let mut state = state.lock().unwrap();
        state.tickets += 1;
        ("200 OK", format!(r#"{{"ticket":"t{}"}}"#, state.tickets))
    } else if method == "DELETE" && target.contains("/terminals/") {
        state.lock().unwrap().closed_terminals.push(target);
        ("200 OK", r#"{"terminals":[]}"#.to_string())
    } else {
        (
            "404 Not Found",
            r#"{"message":"Route not found"}"#.to_string(),
        )
    };
    let response = format!(
        "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes()).await;
}

/// The request line and headers, read without taking them from the stream.
pub async fn peek_head(stream: &TcpStream) -> String {
    let mut buffer = vec![0; 8192];
    loop {
        let read = stream.peek(&mut buffer).await.unwrap();
        let text = String::from_utf8_lossy(&buffer[..read]).to_string();
        if let Some(end) = text.find("\r\n\r\n") {
            return text[..end + 4].to_string();
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

pub fn tasks_json(ids: &[&str]) -> String {
    let tasks: Vec<String> = ids
        .iter()
        .map(|id| {
            format!(
                r#"{{"id":"{id}","title":"Task {id}","state":"running","stage":"implement","workspace":"/w/{id}","updatedAt":"2026-10-22T00:00:00.000Z"}}"#
            )
        })
        .collect();
    format!(r#"{{"tasks":[{}]}}"#, tasks.join(","))
}

pub async fn within<F: Future>(future: F) -> F::Output {
    tokio::time::timeout(WAIT, future).await.expect("timed out")
}

pub fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
}
