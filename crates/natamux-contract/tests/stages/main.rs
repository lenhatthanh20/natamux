//! The stage scenarios of part 3 (23 to 32 of docs/rust-phase4.md), each run on the Node daemon
//! and on the Rust daemon and compared. They use the second project, `/data/stages-repo`, with
//! the pipeline of `tests/fixtures/stages.natamux.json`, and the fake agent
//! (`scripts/fake-agent.sh`) on both daemons.
//!
//! The Rust daemon makes no worktree before part 5. So each scenario makes twin tasks on Node,
//! each with its worktree: `CT-<n>` for the Node run, `CR-<n>` for the Rust run, whose files are
//! copied to the Rust data folder. A new task waits at the first stage (an agent stage), so Node
//! starts nothing for the Rust twin. Ids, worktree paths and commit hashes become placeholders.
//! The daemon images have no `natamux` command, so the fake agent's hooks, its `natamux done` and
//! a test command's `natamux stage pass` reach no daemon. The scenarios report a stage themselves
//! once the agent wrote its file, and the hook scenarios (`hooks.rs`) post the hooks themselves,
//! as `natamux hook claude-code` would.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod finish;
mod flow;
mod hooks;
mod marks;
mod start;

use std::future::Future;
use std::time::{Duration, Instant};

use hyper::Method;
use natamux_contract::daemon::{Answer, Body, Daemon, Token, daemons};
use natamux_contract::transcript::{Allowed, Transcript, compare};
use serde_json::{Map, Value, json};

/// The id of the stages project, from `NATAMUX_CONTRACT_STAGES_REPO`.
pub fn stages_repo() -> String {
    std::env::var("NATAMUX_CONTRACT_STAGES_REPO").unwrap_or_else(|_| "stages-repo".into())
}

/// The id of the twin task of `number` on this daemon.
pub fn twin(daemon: &Daemon, number: u32) -> String {
    match daemon.name {
        "node" => format!("CT-{number}"),
        _ => format!("CR-{number}"),
    }
}

/// The daemon with `task` as the task of its terminal sockets.
pub fn on_task(daemon: &Daemon, task: &str) -> Daemon {
    Daemon {
        task: task.into(),
        ..daemon.clone()
    }
}

/// Makes the twin tasks of `numbers` on Node, each with its worktree, and copies the Rust ones to
/// the Rust daemon. `ready` runs in each worktree once it exists (flag files of a scenario).
pub async fn make_twins(numbers: &[u32], title: &str, ready: &str) {
    let (node, rust) = daemons();
    for number in numbers {
        for id in [format!("CT-{number}"), format!("CR-{number}")] {
            let body = json!({
                "title": format!("{id}: {title}"),
                "repo": stages_repo(),
                "agent": "claude-code",
            });
            let made = post(&node, "/tasks", body).await;
            assert_eq!(made.status, 201, "{id}: {}", made.body);
            post(&node, &format!("/tasks/{id}/workspace"), json!({})).await;
            let workspace = wait_value(&node, &format!("/tasks/{id}"), |task| {
                task["task"]["workspace"].as_str().map(str::to_string)
            })
            .await;
            if !ready.is_empty() {
                node.exec(&format!("cd '{workspace}' && {ready}")).await;
            }
        }
        copy_to_rust(&node, &rust, &format!("CR-{number}")).await;
    }
    rust.restart().await;
}

/// The task files Node wrote, into the Rust data folder. A daemon reads the log of a task file
/// that appears while it runs only at its next start (Node does the same), so the caller
/// restarts the Rust daemon once all are copied.
async fn copy_to_rust(node: &Daemon, rust: &Daemon, id: &str) {
    let (from, to) = (node.data_dir(), rust.data_dir());
    node.exec(&format!(
        "mkdir -p {to}/tasks && cp {from}/tasks/{id}.events.jsonl {from}/tasks/{id}.md {to}/tasks/"
    ))
    .await;
}

/// Runs `scenario` on both daemons, the Node one first, and compares what each saw. The twin
/// task's id, its worktree and its branch become placeholders.
pub async fn both<F, Fut>(name: &str, number: u32, allowed: &[Allowed], scenario: F)
where
    F: Fn(Daemon, String, Transcript) -> Fut,
    Fut: Future<Output = Transcript>,
{
    let (node, rust) = daemons();
    let mut results = Vec::new();
    for daemon in [node, rust] {
        let id = twin(&daemon, number);
        let mut transcript = Transcript::new(daemon.name);
        transcript.alias(&id, "<task>");
        transcript.alias(&id.to_lowercase(), "<task>");
        results.push(scenario(daemon, id, transcript).await);
    }
    compare(name, &results[0], &results[1], allowed);
}

pub async fn call(daemon: &Daemon, method: Method, path: &str, body: Body) -> Answer {
    daemon.request(method, path, body, Token::Daemon).await
}

pub async fn post(daemon: &Daemon, path: &str, body: Value) -> Answer {
    call(daemon, Method::POST, path, Body::Json(body)).await
}

pub async fn put(daemon: &Daemon, path: &str, body: Value) -> Answer {
    call(daemon, Method::PUT, path, Body::Json(body)).await
}

pub async fn get(daemon: &Daemon, path: &str) -> Answer {
    call(daemon, Method::GET, path, Body::None).await
}

/// A 40-digit hex text, a commit.
fn is_sha(text: &str) -> bool {
    text.len() == 40 && text.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// `2026-10-23T09:00:00.000Z` and the like.
fn is_time(text: &str) -> bool {
    let b = text.as_bytes();
    b.len() >= 20 && b[4] == b'-' && b[10] == b'T' && b[b.len() - 1] == b'Z'
}

/// Times become `<time>`, commits `<sha>`; `seq` numbers count from the first of their list.
pub fn clean(value: Value) -> Value {
    match value {
        Value::String(text) if is_time(&text) => json!("<time>"),
        Value::String(text) if is_sha(&text) => json!("<sha>"),
        Value::Array(items) => Value::Array(items.into_iter().map(clean).collect()),
        Value::Object(map) => Value::Object(
            map.into_iter()
                .filter(|(key, _)| key != "seq")
                .map(|(key, each)| (key, clean(each)))
                .collect::<Map<_, _>>(),
        ),
        other => other,
    }
}

/// What a scenario keeps of an answer: the status and the cleaned body.
pub fn kept(answer: &Answer) -> Value {
    json!([answer.status, clean(answer.body.clone())])
}

/// The parts of a task the stage engine moves.
pub fn task_view(task: &Value) -> Value {
    let keep = [
        "stage",
        "state",
        "lanes",
        "paths",
        "skipStages",
        "ciStages",
        "workflowStages",
        "agent",
    ];
    let view: Map<String, Value> = keep
        .iter()
        .filter_map(|key| {
            task.get(*key)
                .map(|value| (key.to_string(), clean(value.clone())))
        })
        .collect();
    Value::Object(view)
}

pub async fn task(daemon: &Daemon, id: &str) -> Value {
    task_view(&get(daemon, &format!("/tasks/{id}")).await.body["task"])
}

/// The task's log as (type, source, payload).
pub async fn stage_events(daemon: &Daemon, id: &str) -> Value {
    let body = get(daemon, &format!("/tasks/{id}/events")).await.body;
    let events: Vec<Value> = body["events"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|entry| &entry["event"])
        .map(|event| {
            json!([
                event["type"],
                event["source"],
                clean(event["payload"].clone()),
                true
            ])
        })
        .collect();
    Value::Array(events)
}

/// Polls `path` until `pick` finds something, at most 120 s (the fake agent pauses 2–5 s per
/// step).
pub async fn wait_value<T>(daemon: &Daemon, path: &str, pick: impl Fn(&Value) -> Option<T>) -> T {
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        let body = get(daemon, path).await.body;
        if let Some(found) = pick(&body) {
            return found;
        }
        assert!(
            Instant::now() < deadline,
            "{}: {path} never got there: {body}",
            daemon.name
        );
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

/// Waits until the task is at `stage` in `state`.
pub async fn wait_at(daemon: &Daemon, id: &str, stage: &str, state: &str) {
    wait_value(daemon, &format!("/tasks/{id}"), |body| {
        let task = &body["task"];
        (task["stage"] == stage && task["state"] == state).then_some(())
    })
    .await;
}

/// Waits until the log has an event of `kind` about `stage`, `count` times.
pub async fn wait_event(daemon: &Daemon, id: &str, kind: &str, stage: &str, count: usize) {
    wait_value(daemon, &format!("/tasks/{id}/events"), |body| {
        let found = body["events"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|entry| {
                entry["event"]["type"] == kind && entry["event"]["payload"]["stage"] == stage
            })
            .count();
        (found >= count).then_some(())
    })
    .await;
}

/// The terminal the last start of `stage` pinned.
pub async fn started_terminal(daemon: &Daemon, id: &str, stage: &str) -> String {
    let body = get(daemon, &format!("/tasks/{id}/events")).await.body;
    body["events"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|entry| &entry["event"])
        .filter(|event| event["type"] == "stage.started" && event["payload"]["stage"] == stage)
        .filter_map(|event| event["payload"]["terminal"].as_str())
        .next_back()
        .unwrap_or("1")
        .to_string()
}

/// Starts the stages that wait (`natamux stage start`): a new task waits at its first stage.
pub async fn start(daemon: &Daemon, id: &str) -> Value {
    act(daemon, id, json!({ "action": "start" })).await
}

/// The report `natamux done` sends from the agent's terminal.
pub async fn report(daemon: &Daemon, id: &str, stage: &str, terminal: &str) -> Value {
    let body = json!({ "action": "report", "stage": stage, "terminal": terminal });
    act(daemon, id, body).await
}

/// Waits until the stage file `name` of the task's worktree holds `needle`.
pub async fn wait_file(daemon: &Daemon, id: &str, name: &str, needle: &str) {
    let path = format!("/data/worktrees/{id}/.natamux-task/{name}");
    let deadline = Instant::now() + Duration::from_secs(120);
    while !daemon
        .exec(&format!("cat '{path}' 2>/dev/null"))
        .await
        .contains(needle)
    {
        assert!(
            Instant::now() < deadline,
            "{}: {path} never held {needle:?}",
            daemon.name
        );
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

/// A stage action of a person.
pub async fn act(daemon: &Daemon, id: &str, body: Value) -> Value {
    kept(&post(daemon, &format!("/tasks/{id}/stage"), body).await)
}

/// An event of a person.
pub async fn event(daemon: &Daemon, id: &str, kind: &str, payload: Value) -> Value {
    let body = json!({ "source": "human", "type": kind, "payload": payload });
    kept(&post(daemon, &format!("/tasks/{id}/events"), body).await)
}

/// The launch script of the agent of `terminal`, as the daemon wrote it.
pub async fn launch_script(daemon: &Daemon, id: &str, terminal: &str) -> Value {
    let suffix = if terminal == "1" {
        String::new()
    } else {
        format!("-{terminal}")
    };
    let path = format!("{}/agent-launch-{id}{suffix}.sh", daemon.data_dir());
    json!(daemon.exec(&format!("cat '{path}' 2>/dev/null")).await)
}

/// The open terminals of a task, their agents and lanes. Programs are the foreground processes,
/// which `ps` lists as it sees them; they are compared too.
pub async fn terminals(daemon: &Daemon, id: &str) -> Value {
    kept(&get(daemon, &format!("/tasks/{id}/terminals")).await)
}

/// Ends a twin, so its agents stop.
pub async fn cancel(daemon: &Daemon, id: &str) {
    event(daemon, id, "task.cancelled", json!({})).await;
}

/// The token a task shell gets: what the agent sends with (part 2's helper, on the part-1 task).
pub async fn agent_token(daemon: &Daemon) -> String {
    use natamux_contract::socket::Socket;
    let mut socket = Socket::open(daemon, "terminal=9&cols=200&rows=20").await;
    socket.until(|socket| socket.count("replayed") > 0).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    socket
        .input("printf 'TOK<%s>\\n' \"$NATAMUX_TOKEN\"\r")
        .await;
    socket
        .until(|socket| socket.text().contains("TOK<") && socket.text().matches('>').count() > 1)
        .await;
    let text = socket.text();
    let token = text
        .rsplit("TOK<")
        .next()
        .and_then(|rest| rest.split('>').next())
        .unwrap_or_default()
        .to_string();
    call(
        daemon,
        Method::DELETE,
        &format!("/tasks/{}/terminals/9", daemon.task),
        Body::None,
    )
    .await;
    token
}
