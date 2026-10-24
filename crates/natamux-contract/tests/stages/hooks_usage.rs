//! 38 `usage` and 39 `hook_time`, in the `hooks` module (the CI filter `hooks::`).

use std::time::Instant;

use natamux_contract::daemon::{Daemon, daemons};
use natamux_contract::events::Events;
use natamux_contract::transcript::Transcript;
use serde_json::{Value, json};

use super::{Agent, running};
use crate::{both, cancel, clean, get, make_twins, stage_events, task, wait_at};

/// A Claude Code session file: `m1` is written twice (the last line counts), a synthetic message
/// and a broken line count for nothing, and one model has no price. Counts are round, so the
/// costs are exact: 1 000 000 input at $2 and 100 000 output at $10 per million.
const FIRST: &str = r#"{"message":{"id":"m1","model":"claude-sonnet-5-5","usage":{"input_tokens":5,"output_tokens":1}}}
{"message":{"id":"m1","model":"claude-sonnet-5-5","usage":{"input_tokens":1000000,"output_tokens":100000,"cache_read_input_tokens":0}}}
{"message":{"id":"s1","model":"<synthetic>","usage":{"input_tokens":999,"output_tokens":999}}}
not json
{"message":{"id":"m2","model":"mystery-model","usage":{"input_tokens":300,"output_tokens":40}}}
"#;

/// A second turn: `m3` is new.
const SECOND: &str = r#"{"message":{"id":"m3","model":"claude-sonnet-5-5","usage":{"input_tokens":500000,"output_tokens":0}}}
"#;

/// What a session file outside the home folder would add, if it were read.
const OUTSIDE: &str = r#"{"message":{"id":"o1","model":"claude-sonnet-5-5","usage":{"input_tokens":7777,"output_tokens":7777}}}
"#;

/// Writes `text` to `path` in the daemon's own container (`append`: after what is there).
async fn write_file(d: &Daemon, path: &str, text: &str, append: bool) {
    let redirect = if append { ">>" } else { ">" };
    let script = format!(
        "mkdir -p \"$(dirname '{path}')\" && cat {redirect} '{path}' <<'NATAMUX_EOF'\n{text}NATAMUX_EOF"
    );
    d.exec_in(&script).await;
}

fn usage_changes(events: &Events, id: &str) -> Vec<Value> {
    events
        .messages
        .iter()
        .filter(|message| message["kind"] == "usage.changed" && message["taskId"] == id)
        .map(|message| clean(message["usage"].clone()))
        .collect()
}

/// Deleting a task answers with the list of all tasks, which differs; only the status counts.
async fn has_usage_file(d: &Daemon, id: &str) -> String {
    let path = format!("{}/tasks/{id}.usage.json", d.data_dir());
    let script = format!("test -e '{path}' && echo yes || echo no");
    d.exec_in(&script).await.trim().to_string()
}

/// The usage of a Claude Code session file, read after a turn end: `usage.changed` and
/// `GET /usage` say the same; a file outside the home folder adds nothing; deleting the task
/// deletes its usage file. Assumes `$HOME` is writable in both containers (the image sets
/// `HOME=/tmp`, a tmpfs) and `/dev/shm` is too; if `/dev/shm` is not, the "outside" file is
/// missing, which also adds nothing.
#[tokio::test]
#[ignore = "needs both daemons: the daemon-contract CI job"]
async fn usage() {
    make_twins(&[3801], "[no-done] Usage", "").await;
    both(
        "usage",
        3801,
        &[],
        |d: Daemon, id, mut t: Transcript| async move {
            running(&d, &id).await;
            let agent = Agent::new(&d, &id).await;
            let home = d.exec_in("printf %s \"$HOME\"").await;
            let session = format!("{home}/.claude/projects/contract/3801.jsonl");
            let outside = "/dev/shm/contract-3801.jsonl";
            write_file(&d, &session, FIRST, false).await;
            write_file(&d, outside, OUTSIDE, false).await;
            let mut events = Events::open(&d, Some(&d.token)).await;
            let stop = |path: &str| json!({ "transcript_path": path });
            t.note("turn end", agent.claude("Stop", stop(&session)).await);
            let changed = |m: &[Value]| {
                m.iter()
                    .any(|m| m["kind"] == "usage.changed" && m["taskId"] == id)
            };
            events.until(changed).await;
            wait_at(&d, &id, "spec", "needs-input").await;
            t.note("usage.changed", Value::Array(usage_changes(&events, &id)));
            let all = get(&d, "/usage").await.body;
            let first = clean(all["usage"][&id].clone());
            t.note("GET /usage", first.clone());
            assert_eq!(vec![first], usage_changes(&events, &id));
            t.note("usage file", json!(has_usage_file(&d, &id).await));
            // Not a session file of the home folder: read by nobody. The next turn shows it.
            agent.claude("UserPromptSubmit", json!({})).await;
            t.note("outside", agent.claude("Stop", stop(outside)).await);
            write_file(&d, &session, SECOND, true).await;
            agent.claude("UserPromptSubmit", json!({})).await;
            t.note(
                "second turn end",
                agent.claude("Stop", stop(&session)).await,
            );
            let twice = |m: &[Value]| {
                let seen = m.iter().filter(|m| m["kind"] == "usage.changed");
                seen.filter(|m| m["taskId"] == id).count() >= 2
            };
            events.until(twice).await;
            t.note(
                "usage.changed again",
                Value::Array(usage_changes(&events, &id)),
            );
            let all = get(&d, "/usage").await.body;
            t.note("GET /usage again", clean(all["usage"][&id].clone()));
            t.note("events", stage_events(&d, &id).await);
            t.note("task", task(&d, &id).await);
            // A delete is left out: the Rust daemon refuses to delete a task that still has a
            // workspace until part 5 (400); its removal of the usage file has a unit test.
            cancel(&d, &id).await;
            t
        },
    )
    .await;
}

fn median(mut times: Vec<u128>) -> u128 {
    times.sort_unstable();
    times[times.len() / 2]
}

/// Milliseconds from sending a hook to the `task.updated` that shows `state` for the task.
async fn time_one(agent: &Agent, events: &mut Events, event: &str, state: &str) -> u128 {
    let started = Instant::now();
    agent.claude(event, json!({})).await;
    let id = agent.id.clone();
    let seen = events
        .next(|m| {
            m["kind"] == "task.updated" && m["task"]["id"] == id && m["task"]["state"] == state
        })
        .await;
    let time = started.elapsed().as_millis();
    assert!(seen, "{}: no {state} for {id} after {event}", agent.d.name);
    time
}

/// Ten hooks on each daemon (`Stop` to needs-input and `UserPromptSubmit` to running, five
/// each), taking turns. Gate: Rust's median is not above Node's. Node is reached through nginx,
/// which is in its number.
#[tokio::test]
#[ignore = "needs both daemons: the daemon-contract CI job"]
async fn hook_time() {
    make_twins(&[3901], "[no-done] Hook time", "").await;
    let (node, rust) = daemons();
    let (node_id, rust_id) = ("CT-3901", "CR-3901");
    running(&node, node_id).await;
    running(&rust, rust_id).await;
    let (a_node, a_rust) = (
        Agent::new(&node, node_id).await,
        Agent::new(&rust, rust_id).await,
    );
    let mut e_node = Events::open(&node, Some(&node.token)).await;
    let mut e_rust = Events::open(&rust, Some(&rust.token)).await;
    let (mut node_times, mut rust_times) = (Vec::new(), Vec::new());
    for _ in 0..5 {
        for (event, state) in [("Stop", "needs-input"), ("UserPromptSubmit", "running")] {
            node_times.push(time_one(&a_node, &mut e_node, event, state).await);
            rust_times.push(time_one(&a_rust, &mut e_rust, event, state).await);
        }
    }
    cancel(&node, node_id).await;
    cancel(&rust, rust_id).await;
    let (node_median, rust_median) = (median(node_times.clone()), median(rust_times.clone()));
    let line = format!(
        "| hook_time | hook POST to its `task.updated` | Node {node_times:?} ms, median {node_median}; Rust {rust_times:?} ms, median {rust_median} |\n"
    );
    print!("{line}");
    if let Ok(path) = std::env::var("NATAMUX_CONTRACT_REPORT") {
        use std::io::Write as _;
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(path)
        {
            let _ = file.write_all(line.as_bytes());
        }
    }
    assert!(
        rust_median <= node_median,
        "a hook is slower on Rust: {rust_median} ms against {node_median} ms"
    );
}
