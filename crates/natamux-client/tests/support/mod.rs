//! An in-process fake daemon for the client's tests: plain HTTP for the routes, and WebSockets
//! whose daemon side each test scripts by hand.

// Each test file uses a part of it.
#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_tungstenite::WebSocketStream;
use async_tungstenite::tokio::{TokioAdapter, accept_async};
use async_tungstenite::tungstenite::Message;
use async_tungstenite::tungstenite::protocol::CloseFrame;
use async_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use futures::StreamExt;
use natamux_client::{Client, ClientError, Config};
use natamux_proto::terminals::messages::ClientMessage;
use serde_json::Value;
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
    /// Answers by "METHOD /target": the status line and the body.
    pub routes: HashMap<String, (String, String)>,
    /// Every request in order: "METHOD /target" and its body.
    pub requests: Vec<(String, String)>,
    /// The request line and headers of each request, in the same order, as sent.
    pub heads: Vec<String>,
    /// How long each answer waits, to keep a read running.
    pub delay: Duration,
}

impl State {
    pub fn answer(&mut self, route: &str, status: &str, body: &str) {
        self.routes
            .insert(route.to_string(), (status.to_string(), body.to_string()));
    }

    /// How many times the client asked for "METHOD /target".
    pub fn count(&self, route: &str) -> usize {
        self.requests
            .iter()
            .filter(|(seen, _)| seen == route)
            .count()
    }

    /// The value of header `name` of the last request.
    pub fn last_header(&self, name: &str) -> Option<String> {
        let head = self.heads.last()?;
        head.lines().find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.eq_ignore_ascii_case(name)
                .then(|| value.trim().to_string())
        })
    }

    pub fn last_request(&self) -> (String, String) {
        self.requests.last().cloned().unwrap_or_default()
    }
}

/// What the dashboard feed reads on every connect, with nothing in it.
const EMPTY_READS: [(&str, &str); 7] = [
    ("GET /repos", r#"{"repos":[],"worktreesDir":"/w"}"#),
    ("GET /jobs", r#"{"jobs":[]}"#),
    ("GET /activity", r#"{"activity":{}}"#),
    ("GET /usage", r#"{"usage":{}}"#),
    ("GET /limits", r#"{"limits":[]}"#),
    ("GET /diffstats", r#"{"stats":{}}"#),
    ("GET /workspaces/preparing", r#"{"preparing":{}}"#),
];

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
        let mut state = State {
            tasks: tasks_json(&["T-1"]),
            ..State::default()
        };
        for (route, body) in EMPTY_READS {
            state.answer(route, "200 OK", body);
        }
        let state = Arc::new(Mutex::new(state));
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
            // The header only: a `?token=` URL is refused (S-15).
            let header = lower.contains(&format!("authorization: bearer {TOKEN}"));
            if !header || target.contains("token=") {
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
    let mut buffer = vec![0; head.len() + content_length(&lower)];
    stream.read_exact(&mut buffer).await.unwrap();
    let body = String::from_utf8_lossy(&buffer[head.len()..]).to_string();
    let route = format!("{method} {target}");
    let delay = {
        let mut state = state.lock().unwrap();
        state.requests.push((route.clone(), body));
        state.heads.push(head.clone());
        state.delay
    };
    tokio::time::sleep(delay).await;
    let authorized = lower.contains(&format!("authorization: bearer {TOKEN}"));
    let (status, body) = if !authorized {
        (
            "401 Unauthorized".to_string(),
            r#"{"error":"unauthorized"}"#.to_string(),
        )
    } else {
        answer(&mut state.lock().unwrap(), &method, &target, &route)
    };
    let response = format!(
        "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes()).await;
}

fn answer(state: &mut State, method: &str, target: &str, route: &str) -> (String, String) {
    let ok = |body: String| ("200 OK".to_string(), body);
    if let Some(answer) = state.routes.get(route).cloned() {
        if route == "GET /tasks" {
            state.task_fetches += 1;
        }
        answer
    } else if route == "GET /tasks" {
        state.task_fetches += 1;
        ok(state.tasks.clone())
    } else if method == "POST" && target.ends_with("/terminal/ticket") {
        state.tickets += 1;
        ok(format!(r#"{{"ticket":"t{}"}}"#, state.tickets))
    } else if method == "DELETE" && target.contains("/terminals/") {
        state.closed_terminals.push(target.to_string());
        ok(r#"{"terminals":[]}"#.to_string())
    } else {
        (
            "404 Not Found".to_string(),
            r#"{"message":"Route not found"}"#.to_string(),
        )
    }
}

fn content_length(lower_head: &str) -> usize {
    lower_head
        .lines()
        .find_map(|line| line.strip_prefix("content-length:"))
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(0)
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

/// Runs one call against a daemon that answers `route` with `body`, and gives back the answer and
/// the body the client sent (Null when it sent none).
pub fn exchange<T: Send + 'static>(
    route: &str,
    status: &str,
    body: &str,
    call: impl FnOnce(&Client) -> natamux_client::Answer<T>,
) -> (Result<T, ClientError>, Value) {
    let rt = runtime();
    let daemon = rt.block_on(FakeDaemon::start());
    daemon.state(|state| state.answer(route, status, body));
    let client = daemon.client();
    let answer = rt.block_on(within(call(&client)));
    let (seen, sent) = daemon.state(|state| state.last_request());
    assert_eq!(seen, route);
    let sent = if sent.is_empty() {
        Value::Null
    } else {
        serde_json::from_str(&sent).unwrap()
    };
    (answer, sent)
}

pub fn ok<T: Send + 'static>(
    route: &str,
    body: &str,
    call: impl FnOnce(&Client) -> natamux_client::Answer<T>,
) -> (T, Value) {
    let (answer, sent) = exchange(route, "200 OK", body, call);
    (answer.unwrap(), sent)
}
