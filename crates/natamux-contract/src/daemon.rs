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

/// The body of a request.
pub enum Body {
    None,
    Json(Value),
    /// A content type (or none) and bytes.
    Raw(Option<&'static str>, Vec<u8>),
}

/// Which token a request carries.
pub enum Token<'a> {
    None,
    Daemon,
    Given(&'a str),
}

/// What a daemon answered.
#[derive(Clone, Debug)]
pub struct Answer {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Value,
}

/// The headers the contract compares.
const KEPT_HEADERS: [&str; 6] = [
    "content-type",
    "content-disposition",
    "cache-control",
    "x-content-type-options",
    "content-security-policy",
    "content-length",
];

fn shell_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
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

    /// One request with any body and token; the status, the headers named in `keep`, and the body
    /// as JSON (or as text in `{"text": ...}` when it is not JSON).
    pub async fn request(
        &self,
        method: Method,
        path: &str,
        body: Body,
        token: Token<'_>,
    ) -> Answer {
        self.request_with(method, path, body, token, &[]).await
    }

    /// Like `request`, with more headers (the task and terminal headers of a hook).
    pub async fn request_with(
        &self,
        method: Method,
        path: &str,
        body: Body,
        token: Token<'_>,
        headers: &[(&str, &str)],
    ) -> Answer {
        let (host, prefix) = split(&self.base);
        let failed = Answer {
            status: 0,
            headers: Vec::new(),
            body: Value::Null,
        };
        let Ok(stream) = TcpStream::connect(&host).await else {
            return failed;
        };
        let Ok((mut sender, connection)) =
            hyper::client::conn::http1::handshake(TokioIo::new(stream)).await
        else {
            return failed;
        };
        tokio::spawn(connection);
        let mut request = Request::builder()
            .method(method)
            .uri(format!("{prefix}{path}"))
            .header("host", host.as_str());
        match token {
            Token::Daemon => {
                request = request.header("authorization", format!("Bearer {}", self.token))
            }
            Token::Given(token) => {
                request = request.header("authorization", format!("Bearer {token}"))
            }
            Token::None => {}
        }
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        let bytes = match body {
            Body::None => Vec::new(),
            Body::Json(value) => {
                request = request.header("content-type", "application/json");
                value.to_string().into_bytes()
            }
            Body::Raw(kind, bytes) => {
                if let Some(kind) = kind {
                    request = request.header("content-type", kind);
                }
                bytes
            }
        };
        let Ok(request) = request.body(Full::new(Bytes::from(bytes))) else {
            return failed;
        };
        let Ok(answer) = sender.send_request(request).await else {
            return failed;
        };
        let status = answer.status().as_u16();
        let headers = KEPT_HEADERS
            .iter()
            .filter_map(|name| {
                let value = answer.headers().get(*name)?.to_str().ok()?;
                Some((name.to_string(), value.to_string()))
            })
            .collect();
        let raw = answer
            .into_body()
            .collect()
            .await
            .map(|body| body.to_bytes())
            .unwrap_or_default();
        let body = serde_json::from_slice(&raw)
            .unwrap_or_else(|_| serde_json::json!({ "text": String::from_utf8_lossy(&raw) }));
        Answer {
            status,
            headers,
            body,
        }
    }

    /// Runs `script` in the daemon container (both share the data volume), and gives its output.
    pub async fn exec(&self, script: &str) -> String {
        let command = format!(
            "{} sh -c {}",
            env("NATAMUX_CONTRACT_EXEC"),
            shell_quote(script)
        );
        let output = tokio::task::spawn_blocking(move || {
            std::process::Command::new("sh")
                .args(["-c", &command])
                .output()
        })
        .await;
        match output {
            Ok(Ok(output)) => String::from_utf8_lossy(&output.stdout).into_owned(),
            _ => String::new(),
        }
    }

    /// Runs `script` in this daemon's own container: Node's is `NATAMUX_CONTRACT_EXEC`, Rust's is
    /// `NATAMUX_CONTRACT_RUST_EXEC`, which only the scenarios that need it require.
    pub async fn exec_in(&self, script: &str) -> String {
        let name = if self.name == "node" {
            "NATAMUX_CONTRACT_EXEC"
        } else {
            "NATAMUX_CONTRACT_RUST_EXEC"
        };
        let command = format!("{} sh -c {}", env(name), shell_quote(script));
        let output = tokio::task::spawn_blocking(move || {
            std::process::Command::new("sh")
                .args(["-c", &command])
                .output()
        })
        .await;
        match output {
            Ok(Ok(output)) => String::from_utf8_lossy(&output.stdout).into_owned(),
            _ => String::new(),
        }
    }

    /// The folder of this daemon's fake `gh` state (`scripts/fake-gh.sh`).
    fn fake_gh_dir(&self) -> &'static str {
        if self.name == "node" {
            "/data/fake-gh/node"
        } else {
            "/data/fake-gh/rust"
        }
    }

    /// The `gh` calls this daemon made, one per line; empty when there are none.
    pub async fn fake_gh_calls(&self) -> String {
        let path = format!("{}/calls.log", self.fake_gh_dir());
        self.exec_in(&format!("cat '{path}' 2>/dev/null")).await
    }

    /// Writes `text` to the file `name` in this daemon's fake `gh` folder (`pr-state.json`, ...).
    pub async fn write_fake_gh(&self, name: &str, text: &str) {
        let dir = self.fake_gh_dir();
        let script = format!(
            "mkdir -p '{dir}' && cat > '{dir}/{name}' <<'NATAMUX_EOF'\n{text}\nNATAMUX_EOF"
        );
        self.exec_in(&script).await;
    }

    /// The output of `git -C <dir> <args>` in this daemon's container.
    pub async fn git_in(&self, dir: &str, args: &str) -> String {
        self.exec_in(&format!("git -C '{dir}' {args} 2>&1")).await
    }

    /// The data folder of this daemon inside the container.
    pub fn data_dir(&self) -> String {
        env(if self.name == "node" {
            "NATAMUX_CONTRACT_NODE_DATA"
        } else {
            "NATAMUX_CONTRACT_RUST_DATA"
        })
    }

    /// The `/events` URL with the token in the query.
    pub fn events_url(&self, token: Option<&str>) -> String {
        let base = self.base.replacen("http://", "ws://", 1);
        match token {
            Some(token) => format!("{base}/events?token={token}"),
            None => format!("{base}/events"),
        }
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
