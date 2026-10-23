//! A daemon on a free port with pseudo terminals that echo, and an HTTP and WebSocket client for
//! it (the setup of `server.test.ts`).

use std::path::PathBuf;
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_tungstenite::WebSocketStream;
use async_tungstenite::tokio::{ConnectStream, connect_async};
use async_tungstenite::tungstenite::Message;
use futures::StreamExt;
use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper::{Method, Request};
use hyper_util::rt::TokioIo;
use natamux_daemon::config::Config;
use natamux_daemon::shared::scheduler::Scheduler;
use natamux_daemon::start::{Daemon, start};
use natamux_pty::{Pty, PtyEvents, Signal, SpawnOptions};
use serde_json::Value;
use tokio::net::TcpStream;

pub const TOKEN: &str = "test-token";

enum Event {
    Data(Vec<u8>),
    Exit(Option<i32>),
}

/// What a test sees of one echo pty.
#[derive(Clone)]
pub struct EchoPty {
    pub options: Arc<SpawnOptions>,
    pub written: Arc<Mutex<Vec<String>>>,
    pub sizes: Arc<Mutex<Vec<(u16, u16)>>>,
    events: Sender<Event>,
}

impl EchoPty {
    pub fn emit(&self, text: &str) {
        let _ = self.events.send(Event::Data(text.as_bytes().to_vec()));
    }

    pub fn exit(&self, code: Option<i32>) {
        let _ = self.events.send(Event::Exit(code));
    }

    pub fn sizes(&self) -> Vec<(u16, u16)> {
        self.sizes.lock().unwrap().clone()
    }
}

struct EchoSide {
    handle: EchoPty,
    receiver: Option<std::sync::mpsc::Receiver<Event>>,
}

impl Pty for EchoSide {
    fn start(&mut self, events: PtyEvents) {
        let Some(receiver) = self.receiver.take() else {
            return;
        };
        let PtyEvents {
            mut on_data,
            on_exit,
        } = events;
        std::thread::spawn(move || {
            for event in receiver {
                match event {
                    Event::Data(data) => on_data(data),
                    Event::Exit(code) => {
                        on_exit(code);
                        return;
                    }
                }
            }
        });
    }

    fn write(&mut self, data: &[u8]) {
        let text = String::from_utf8_lossy(data).into_owned();
        self.handle.written.lock().unwrap().push(text);
        let _ = self.handle.events.send(Event::Data(data.to_vec()));
    }

    fn resize(&mut self, cols: u16, rows: u16) {
        self.handle.sizes.lock().unwrap().push((cols, rows));
    }

    fn pause(&mut self) {}

    fn resume(&mut self) {}

    fn kill(&mut self, _signal: Signal) {
        let _ = self.handle.events.send(Event::Exit(None));
    }

    fn foreground(&self) -> Option<String> {
        Some("bash".into())
    }
}

pub struct TestDaemon {
    pub daemon: Daemon,
    pub ptys: Arc<Mutex<Vec<EchoPty>>>,
    pub root: PathBuf,
}

impl Drop for TestDaemon {
    fn drop(&mut self) {
        self.daemon.server.abort();
        self.daemon.scheduler.stop();
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// A daemon whose data folder is `<root>/data`; `with_workspace` makes T-0001's worktree.
pub async fn daemon(name: &str, with_workspace: bool) -> TestDaemon {
    daemon_in(test_root(name), with_workspace).await
}

pub fn test_root(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("natamux-routes-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("data")).unwrap();
    root
}

pub async fn daemon_in(root: PathBuf, with_workspace: bool) -> TestDaemon {
    daemon_silent_after(
        root,
        with_workspace,
        natamux_proto::terminals::messages::SILENT_CLOSE,
    )
    .await
}

/// A daemon that drops a socket after `silent_close` with no message (90 s in production).
pub async fn daemon_silent_after(
    root: PathBuf,
    with_workspace: bool,
    silent_close: Duration,
) -> TestDaemon {
    if with_workspace {
        std::fs::create_dir_all(root.join("data/worktrees/T-0001")).unwrap();
    }
    let config = Config {
        host: "127.0.0.1".into(),
        port: 0,
        data_dir: root.join("data"),
        worktrees_dir: root.join("data/worktrees"),
        token: TOKEN.into(),
        token_was_generated: false,
        agent_token: "agent-token".into(),
        silent_close,
    };
    let ptys: Arc<Mutex<Vec<EchoPty>>> = Arc::default();
    let made = Arc::clone(&ptys);
    let spawn = move |options: SpawnOptions| -> std::io::Result<Box<dyn Pty>> {
        let (events, receiver) = channel();
        let handle = EchoPty {
            options: Arc::new(options),
            written: Arc::default(),
            sizes: Arc::default(),
            events,
        };
        made.lock().unwrap().push(handle.clone());
        Ok(Box::new(EchoSide {
            handle,
            receiver: Some(receiver),
        }))
    };
    let env = [("PATH", "/bin")]
        .iter()
        .map(|(name, value)| (name.to_string(), value.to_string()))
        .collect();
    let daemon = start(&config, &env, Arc::new(spawn), Scheduler::real())
        .await
        .unwrap();
    TestDaemon { daemon, ptys, root }
}

impl TestDaemon {
    pub fn ptys(&self) -> Vec<EchoPty> {
        self.ptys.lock().unwrap().clone()
    }

    /// One request; `token` adds the bearer header. Gives the status and the JSON body (Null
    /// for an empty or non-JSON one).
    pub async fn call(&self, method: Method, path: &str, token: Option<&str>) -> (u16, Value) {
        let stream = TcpStream::connect(self.daemon.address).await.unwrap();
        let (mut sender, connection) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
            .await
            .unwrap();
        tokio::spawn(connection);
        let mut request = Request::builder()
            .method(method)
            .uri(path)
            .header("host", "localhost");
        if let Some(token) = token {
            request = request.header("authorization", format!("Bearer {token}"));
        }
        let answer = sender
            .send_request(request.body(Full::new(Bytes::new())).unwrap())
            .await
            .unwrap();
        let status = answer.status().as_u16();
        let body = answer.into_body().collect().await.unwrap().to_bytes();
        (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
    }

    /// The status of a request sent as raw bytes, so a path with "\\" reaches the daemon as it is.
    pub async fn raw_status(&self, method: &str, path: &str) -> u16 {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut stream = TcpStream::connect(self.daemon.address).await.unwrap();
        let request =
            format!("{method} {path} HTTP/1.1\r\nhost: localhost\r\nconnection: close\r\n\r\n");
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut answer = Vec::new();
        let _ = stream.read_to_end(&mut answer).await;
        let text = String::from_utf8_lossy(&answer);
        text.split_whitespace()
            .nth(1)
            .and_then(|code| code.parse().ok())
            .unwrap_or(0)
    }

    pub async fn ticket(&self) -> String {
        let (status, body) = self
            .call(Method::POST, "/tasks/T-0001/terminal/ticket", Some(TOKEN))
            .await;
        assert_eq!(status, 200, "{body}");
        body["ticket"].as_str().unwrap().to_string()
    }

    /// A terminal socket of T-0001 with a fresh ticket and `query`.
    pub async fn open(&self, query: &str) -> Socket {
        let ticket = self.ticket().await;
        self.open_with(&format!("ticket={ticket}&{query}")).await
    }

    pub async fn open_with(&self, query: &str) -> Socket {
        let url = format!("ws://{}/tasks/T-0001/terminal?{query}", self.daemon.address);
        let (socket, _) = connect_async(url.as_str()).await.unwrap();
        Socket {
            socket,
            frames: Vec::new(),
            closed: None,
        }
    }
}

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
