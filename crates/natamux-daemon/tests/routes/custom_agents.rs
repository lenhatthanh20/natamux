//! `GET`/`PUT /custom-agents`, and starting a custom agent: a Claude Code variant with its own
//! command, and another CLI. Unix only: Windows types a launch in full, with no script.

use std::os::unix::fs::PermissionsExt;

use hyper::Method;
use serde_json::{Value, json};

use crate::server::{TOKEN, TestDaemon, start_daemon, test_root};

const PATH: &str = "/custom-agents";

fn qwen_and_other() -> Value {
    json!({ "agents": [
        { "id": "claude-qwen", "base": "claude-code", "command": "cq" },
        { "id": "my-agent", "base": "other", "command": "my-agent" },
    ]})
}

/// The command of a launch script, without the PATH it sets first.
fn launch_line(d: &TestDaemon, id: &str) -> String {
    let script =
        std::fs::read_to_string(d.root.join(format!("data/agent-launch-{id}.sh"))).unwrap();
    let line = script.lines().last().unwrap_or("").to_string();
    match line.split_once(' ') {
        Some((first, rest)) if first.starts_with("PATH=") => rest.to_string(),
        _ => line,
    }
}

// One test: the list is the process's, so no other test of this binary may change it.
#[tokio::test(flavor = "multi_thread")]
async fn saves_the_custom_agents_and_starts_them() {
    let root = test_root("custom-agents");
    let file = root.join("data/custom-agents.json");
    std::fs::write(&file, "{ broken").unwrap();
    let d = start_daemon(
        root,
        None,
        natamux_proto::terminals::messages::SILENT_CLOSE,
        None,
    )
    .await;

    // A broken file: the daemon starts with no custom agents.
    let (status, body) = d.call(Method::GET, PATH, Some(TOKEN)).await;
    assert_eq!((status, body), (200, json!({ "agents": [] })));

    // A bad list is refused and the file stays as it was.
    let taken = json!({ "agents": [{ "id": "claude-code", "base": "codex", "command": "x" }] });
    let (status, body) = d.send(Method::PUT, PATH, Some(TOKEN), &taken).await;
    assert_eq!(status, 400, "{body}");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "{ broken");

    // An agent may not name a program to run.
    d.make_task("Fix the bug").await;
    d.give_workspace("T-0001");
    let agent = d.agent_token("T-0001");
    let (status, _) = d
        .send(Method::PUT, PATH, Some(&agent), &qwen_and_other())
        .await;
    assert_eq!(status, 403);

    let (status, body) = d
        .send(Method::PUT, PATH, Some(TOKEN), &qwen_and_other())
        .await;
    assert_eq!((status, body), (200, qwen_and_other()));
    let (_, listed) = d.call(Method::GET, PATH, Some(TOKEN)).await;
    assert_eq!(listed, qwen_and_other());
    let mode = std::fs::metadata(&file).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o600);

    // The Claude Code variant starts as Claude Code, with `cq` in place of `claude`.
    let start = json!({ "agent": "claude-qwen" });
    let (status, body) = d
        .send(Method::POST, "/tasks/T-0001/agent", Some(TOKEN), &start)
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["task"]["agent"], "claude-qwen");
    let line = launch_line(&d, "T-0001");
    assert!(line.starts_with("cq "), "{line}");
    assert!(line.ends_with("'Fix the bug'"), "{line}");

    // Another CLI gets the prompt as its last word.
    d.make_task("Write docs").await;
    d.give_workspace("T-0002");
    let start = json!({ "agent": "my-agent" });
    let (status, body) = d
        .send(Method::POST, "/tasks/T-0002/agent", Some(TOKEN), &start)
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(launch_line(&d, "T-0002"), "my-agent 'Write docs'");

    // An unknown id names every agent that can start.
    let start = json!({ "agent": "aider" });
    let (status, body) = d
        .send(Method::POST, "/tasks/T-0002/agent", Some(TOKEN), &start)
        .await;
    assert_eq!(status, 404);
    let text = body.to_string();
    assert!(text.contains("cursor, claude-qwen, my-agent"), "{text}");

    // A saved change counts for the next start, with no restart.
    let renamed = json!({ "agents": [
        { "id": "my-agent", "base": "other", "command": "my-agent2" },
    ]});
    d.send(Method::PUT, PATH, Some(TOKEN), &renamed).await;
    d.make_task("Write more docs").await;
    d.give_workspace("T-0003");
    let start = json!({ "agent": "my-agent" });
    d.send(Method::POST, "/tasks/T-0003/agent", Some(TOKEN), &start)
        .await;
    assert_eq!(launch_line(&d, "T-0003"), "my-agent2 'Write more docs'");
}
