//! A daemon under test, found from the environment of the CI job.

use std::time::{Duration, Instant};

use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper::{Method, Request};
use hyper_util::rt::TokioIo;
use serde_json::Value;
use tokio::net::TcpStream;

/// One daemon: its base URL (with `/api` for the Node one behind nginx), its token, and the
/// shell command that restarts it.
#[derive(Clone, Debug)]
pub struct Daemon {
    pub name: &'static str,
    pub base: String,
    pub token: String,
    pub restart: String,
    pub task: String,
}

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| {
        eprintln!("{name} is not set: the contract suite runs in the daemon-contract CI job");
        std::process::exit(1)
    })
}

/// The Node daemon and the Rust daemon, from `NATAMUX_CONTRACT_*`.
pub fn daemons() -> (Daemon, Daemon) {
    let token = env("NATAMUX_CONTRACT_TOKEN");
    let task = env("NATAMUX_CONTRACT_TASK");
    let daemon = |name, base: &str, restart: &str| Daemon {
        name,
        base: env(base),
        token: token.clone(),
        restart: env(restart),
        task: task.clone(),
    };
    (
        daemon(
            "node",
            "NATAMUX_CONTRACT_NODE",
            "NATAMUX_CONTRACT_NODE_RESTART",
        ),
        daemon(
            "rust",
            "NATAMUX_CONTRACT_RUST",
            "NATAMUX_CONTRACT_RUST_RESTART",
        ),
    )
}

/// `http://host:port/path` cut in its address and path.
fn split(url: &str) -> (String, String) {
    let rest = url.trim_start_matches("http://");
    match rest.split_once('/') {
        Some((host, path)) => (host.to_string(), format!("/{path}")),
        None => (rest.to_string(), String::new()),
    }
}

impl Daemon {
    /// One request, with the bearer token when `token` is set. The status and the JSON body
    /// (Null for an empty or non-JSON one).
    pub async fn call(&self, method: Method, path: &str, token: bool) -> (u16, Value) {
        let (host, prefix) = split(&self.base);
        let Ok(stream) = TcpStream::connect(&host).await else {
            return (0, Value::Null);
        };
        let Ok((mut sender, connection)) =
            hyper::client::conn::http1::handshake(TokioIo::new(stream)).await
        else {
            return (0, Value::Null);
        };
        tokio::spawn(connection);
        let mut request = Request::builder()
            .method(method)
            .uri(format!("{prefix}{path}"))
            .header("host", host.as_str());
        if token {
            request = request.header("authorization", format!("Bearer {}", self.token));
        }
        let Ok(request) = request.body(Full::new(Bytes::new())) else {
            return (0, Value::Null);
        };
        let Ok(answer) = sender.send_request(request).await else {
            return (0, Value::Null);
        };
        let status = answer.status().as_u16();
        let body = answer
            .into_body()
            .collect()
            .await
            .map(|body| body.to_bytes())
            .unwrap_or_default();
        (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
    }

    pub async fn ticket(&self) -> String {
        let path = format!("/tasks/{}/terminal/ticket", self.task);
        let (_, body) = self.call(Method::POST, &path, true).await;
        body["ticket"].as_str().unwrap_or_default().to_string()
    }

    /// The WebSocket URL of the terminal of the task with this query.
    pub fn socket_url(&self, query: &str) -> String {
        let base = self.base.replacen("http://", "ws://", 1);
        format!("{base}/tasks/{}/terminal?{query}", self.task)
    }

    /// Closes every shell of the task, so a scenario starts from none.
    pub async fn close_all(&self) {
        let path = format!("/tasks/{}/terminals", self.task);
        let (_, body) = self.call(Method::GET, &path, true).await;
        for name in body["terminals"].as_array().into_iter().flatten() {
            if let Some(name) = name.as_str() {
                self.call(Method::DELETE, &format!("{path}/{name}"), true)
                    .await;
            }
        }
    }

    /// Restarts the daemon and waits until it answers again.
    pub async fn restart(&self) {
        let command = self.restart.clone();
        let status = tokio::task::spawn_blocking(move || {
            std::process::Command::new("sh")
                .args(["-c", &command])
                .status()
        })
        .await;
        let ok = matches!(status, Ok(Ok(status)) if status.success());
        assert!(ok, "restart of {} failed", self.name);
        let deadline = Instant::now() + Duration::from_secs(60);
        while self.call(Method::GET, "/health", false).await.0 != 200 {
            assert!(Instant::now() < deadline, "{} did not come back", self.name);
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }
}
