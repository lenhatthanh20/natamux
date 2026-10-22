//! Helpers shared by the integration tests: a recording fake daemon, the captured output and a
//! task like `makeTask` of the TypeScript tests.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};

use natamux_cli::daemon::Transport;
use natamux_cli::run::{Env, Io, run_cli};
use natamux_client::Method;
use natamux_proto::tasks::task::Task;
use serde_json::{Value, json};

/// One request the command sent. `url` is the base address of `connect` plus the path.
#[derive(Clone, Debug)]
pub struct Call {
    pub url: String,
    pub method: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
    /// The real transport adds the bearer header from this.
    pub token: String,
}

impl Call {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    /// The body as JSON, `None` when there is no body.
    pub fn json(&self) -> Option<Value> {
        self.body
            .as_ref()
            .map(|body| serde_json::from_slice(body).unwrap())
    }

    pub fn body_text(&self) -> String {
        String::from_utf8(self.body.clone().unwrap_or_default()).unwrap()
    }
}

pub enum Reply {
    Json(u16, Value),
    Raw(u16, Vec<u8>),
    /// The daemon cannot be reached.
    Down,
    /// The daemon takes the request and never answers.
    Never,
}

pub fn json_reply(value: Value) -> Reply {
    Reply::Json(200, value)
}

pub fn json_status(status: u16, value: Value) -> Reply {
    Reply::Json(status, value)
}

pub fn raw_reply(status: u16, text: &str) -> Reply {
    Reply::Raw(status, text.as_bytes().to_vec())
}

type Handler = Rc<dyn Fn(&Call) -> Reply>;

struct Fake {
    base_url: String,
    token: String,
    calls: Rc<RefCell<Vec<Call>>>,
    handler: Handler,
}

impl Transport for Fake {
    async fn request(
        &self,
        method: Method,
        path: &str,
        headers: &[(&str, &str)],
        body: Option<Vec<u8>>,
    ) -> Result<(u16, Vec<u8>), String> {
        let call = Call {
            url: format!("{}{path}", self.base_url),
            method: method.to_string(),
            headers: headers
                .iter()
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect(),
            body,
            token: self.token.clone(),
        };
        let reply = (self.handler)(&call);
        self.calls.borrow_mut().push(call);
        match reply {
            Reply::Json(status, value) => Ok((status, serde_json::to_vec(&value).unwrap())),
            Reply::Raw(status, bytes) => Ok((status, bytes)),
            Reply::Down => Err("fetch failed".into()),
            Reply::Never => std::future::pending().await,
        }
    }
}

#[derive(Default)]
pub struct Captured {
    pub out: String,
    pub err: String,
}

impl Io for Captured {
    fn stdout(&mut self, text: &str) {
        self.out.push_str(text);
    }

    fn stderr(&mut self, text: &str) {
        self.err.push_str(text);
    }
}

pub struct Outcome {
    pub code: u8,
    pub stdout: String,
    pub stderr: String,
    pub calls: Vec<Call>,
}

impl Outcome {
    /// "METHOD url" of every call.
    pub fn lines(&self) -> Vec<String> {
        self.calls
            .iter()
            .map(|call| format!("{} {}", call.method, call.url))
            .collect()
    }
}

pub const TOKEN_ENV: [(&str, &str); 1] = [("NATAMUX_TOKEN", "secret")];

/// Runs the command with the token set and an empty stdin.
pub fn run(argv: &[&str], handler: impl Fn(&Call) -> Reply + 'static) -> Outcome {
    run_with(argv, Env::from(TOKEN_ENV), b"", handler)
}

/// A command that never reaches the daemon (an empty answer if it does).
pub fn run_plain(argv: &[&str], env: Env) -> Outcome {
    run_with(argv, env, b"", |_| json_reply(json!({})))
}

pub fn run_with(
    argv: &[&str],
    env: Env,
    stdin: &[u8],
    handler: impl Fn(&Call) -> Reply + 'static,
) -> Outcome {
    let argv: Vec<String> = argv.iter().map(|arg| arg.to_string()).collect();
    let calls = Rc::new(RefCell::new(Vec::new()));
    let handler: Handler = Rc::new(handler);
    let mut io = Captured::default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let code = runtime.block_on(run_cli(
        &argv,
        &env,
        &mut io,
        &|| stdin.to_vec(),
        &|base_url: &str, token: &str| Fake {
            base_url: base_url.to_string(),
            token: token.to_string(),
            calls: calls.clone(),
            handler: handler.clone(),
        },
    ));
    let calls = calls.borrow().clone();
    Outcome {
        code,
        stdout: io.out,
        stderr: io.err,
        calls,
    }
}

pub fn make_task() -> Task {
    Task {
        id: "T-0001".into(),
        title: "Fix the dashboard".into(),
        priority: "normal".into(),
        stage: "implement".into(),
        stages: ["implement", "review", "release"].map(Value::from).to_vec(),
        created_at: "2026-10-08T09:00:00.000Z".into(),
        updated_at: "2026-10-08T09:00:00.000Z".into(),
        ..Task::default()
    }
}

pub fn make_task_with(change: impl FnOnce(&mut Task)) -> Task {
    let mut task = make_task();
    change(&mut task);
    task
}

/// `{"task": ...}` as the daemon answers.
pub fn task_reply(task: &Task) -> Reply {
    json_reply(json!({ "task": task }))
}

pub fn task_created(task: &Task) -> Reply {
    json_status(201, json!({ "task": task }))
}

/// `task create` without --repo asks for the projects first; with one project, it uses that one.
pub fn one_project(handler: impl Fn(&Call) -> Reply + 'static) -> impl Fn(&Call) -> Reply {
    move |call| {
        if call.url.ends_with("/repos") {
            json_reply(json!({ "repos": [{ "id": "app", "name": "app", "path": "/work/app" }] }))
        } else {
            handler(call)
        }
    }
}

/// `expected` is a part of `actual`: the same keys and values, and `actual` may have more keys.
pub fn assert_has(actual: &Value, expected: &Value) {
    fn has(actual: &Value, expected: &Value) -> bool {
        match (actual, expected) {
            (Value::Object(actual), Value::Object(expected)) => expected
                .iter()
                .all(|(key, value)| actual.get(key).is_some_and(|found| has(found, value))),
            _ => actual == expected,
        }
    }
    assert!(has(actual, expected), "{actual} does not have {expected}");
}

/// Splits every line of a printed table into its cells, so tests do not depend on the padding.
pub fn cells(output: &str) -> Vec<Vec<String>> {
    output.trim_end().lines().map(split_cells).collect()
}

fn split_cells(line: &str) -> Vec<String> {
    let mut cells = Vec::new();
    let mut cell = String::new();
    let mut spaces = 0;
    for character in line.chars() {
        if character == ' ' {
            spaces += 1;
            continue;
        }
        if spaces >= 2 {
            cells.push(std::mem::take(&mut cell));
        } else {
            cell.push_str(&" ".repeat(spaces));
        }
        spaces = 0;
        cell.push(character);
    }
    cells.push(cell);
    cells
}

/// A folder under the system temp folder that goes away with the value.
pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new(name: &str) -> TempDir {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!(
            "natamux-{name}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).unwrap();
        TempDir(path)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    pub fn join(&self, name: &str) -> String {
        self.0.join(name).to_string_lossy().into_owned()
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
