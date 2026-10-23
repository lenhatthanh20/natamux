//! A daemon on a free port with pseudo terminals that echo, and an HTTP and WebSocket client for
//! it (the setup of `server.test.ts`).

use std::path::PathBuf;
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_tungstenite::tokio::connect_async;
use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper::{Method, Request};
use hyper_util::rt::TokioIo;
use natamux_daemon::config::Config;
use natamux_daemon::shared::scheduler::Scheduler;
use natamux_daemon::start::{Daemon, start};
use natamux_proto::tasks::task::Task;
use natamux_pty::{Pty, PtyEvents, Signal, SpawnOptions};
use natamux_store::task_files::write_task_file;
use serde_json::{Value, json};
use tokio::net::TcpStream;

pub const TOKEN: &str = "test-token";
pub const AGENT_TOKEN: &str = "agent-token";

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
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let root =
        std::env::temp_dir().join(format!("natamux-routes-{name}-{}-{n}", std::process::id()));
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

/// A daemon with the default repository and no task: the tasks of a test are made through it.
pub async fn bare(name: &str) -> TestDaemon {
    start_daemon(
        test_root(name),
        None,
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
    start_daemon(root, Some(with_workspace), silent_close).await
}

/// T-0001 is a task file in `<data>/tasks`; its workspace is the worktree folder, or none.
fn seed_task(root: &std::path::Path, with_workspace: bool) {
    let worktree = root.join("data/worktrees/T-0001");
    let workspace = with_workspace.then(|| {
        std::fs::create_dir_all(&worktree).unwrap();
        worktree.to_string_lossy().into_owned()
    });
    let task: Task = serde_json::from_value(json!({
        "id": "T-0001",
        "title": "Example",
        "state": if with_workspace { "running" } else { "draft" },
        "workspace": workspace,
        "stage": "implement",
        "stages": ["implement", "review", "release"],
        "createdAt": "2026-10-15T00:00:00.000Z",
        "updatedAt": "2026-10-15T00:00:00.000Z",
    }))
    .unwrap();
    write_task_file(&root.join("data"), &task).unwrap();
}

/// The default repository, as `repos.json` stores it; its folder is not a git repository.
fn seed_repos(root: &std::path::Path) {
    let folder = root.join("repo");
    std::fs::create_dir_all(&folder).unwrap();
    let repos = json!([{ "id": "default", "name": "default", "path": folder }]);
    std::fs::write(root.join("data/repos.json"), repos.to_string()).unwrap();
}

/// `task`: T-0001 is seeded, with a workspace or without; None: no task.
async fn start_daemon(root: PathBuf, task: Option<bool>, silent_close: Duration) -> TestDaemon {
    seed_repos(&root);
    if let Some(with_workspace) = task {
        seed_task(&root, with_workspace);
    }
    let config = Config {
        host: "127.0.0.1".into(),
        port: 0,
        data_dir: root.join("data"),
        worktrees_dir: root.join("data/worktrees"),
        token: TOKEN.into(),
        token_was_generated: false,
        agent_token: AGENT_TOKEN.into(),
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

    /// One request with a JSON body.
    pub async fn send(
        &self,
        method: Method,
        path: &str,
        token: Option<&str>,
        body: &Value,
    ) -> (u16, Value) {
        let (status, _, body) = self.exchange(method, path, token, Some(body)).await;
        (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
    }

    /// The status, the headers and the body bytes of a request.
    pub async fn exchange(
        &self,
        method: Method,
        path: &str,
        token: Option<&str>,
        body: Option<&Value>,
    ) -> (u16, hyper::HeaderMap, Vec<u8>) {
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
        let payload = body.map_or_else(Bytes::new, |body| Bytes::from(body.to_string()));
        if body.is_some() {
            request = request.header("content-type", "application/json");
        }
        let answer = sender
            .send_request(request.body(Full::new(payload)).unwrap())
            .await
            .unwrap();
        let status = answer.status().as_u16();
        let headers = answer.headers().clone();
        let bytes = answer.into_body().collect().await.unwrap().to_bytes();
        (status, headers, bytes.to_vec())
    }

    /// What removing the workspace does to T-0001: the task file keeps none, and the daemon reads
    /// the file again.
    pub async fn clear_workspace(&self) {
        let (_, body) = self.call(Method::GET, "/tasks/T-0001", Some(TOKEN)).await;
        let mut task: Task = serde_json::from_value(body["task"].clone()).unwrap();
        task.workspace = None;
        write_task_file(&self.root.join("data"), &task).unwrap();
        for _ in 0..500 {
            let (_, body) = self.call(Method::GET, "/tasks/T-0001", Some(TOKEN)).await;
            if body["task"]["workspace"].is_null() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("the daemon did not read the task file again");
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

    /// A dashboard on `/events`, with `?token=` as a browser has to send it.
    pub async fn dashboard(&self, token: &str) -> Socket {
        let url = format!("ws://{}/events?token={token}", self.daemon.address);
        let (socket, _) = connect_async(url.as_str()).await.unwrap();
        Socket::new(socket)
    }

    pub async fn open_with(&self, query: &str) -> Socket {
        let url = format!("ws://{}/tasks/T-0001/terminal?{query}", self.daemon.address);
        let (socket, _) = connect_async(url.as_str()).await.unwrap();
        Socket::new(socket)
    }
}

pub use crate::socket::{Socket, eventually};
