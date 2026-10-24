//! The agent cases of `server.test.ts`: 'shows the shell of an earlier stage as done', 'sends a
//! review comment', 'start an agent' and 'reopen the agent of a finished task', with their hooks.

use hyper::Method;
use natamux_daemon::terminals::shell_files::save_ended_shells;
use natamux_proto::terminals::ended::EndedShell;
use serde_json::json;

use crate::git_repo::git_daemon;
use crate::server::{TOKEN, TestDaemon, bare, daemon, eventually};

const REOPEN: &str = "/tasks/T-0001/agent/reopen";

#[tokio::test(flavor = "multi_thread")]
async fn shows_the_shell_of_an_earlier_stage_as_done_until_its_agent_gets_a_new_prompt() {
    let d = bare("lane").await;
    d.make_task("Fix the bug").await;
    d.give_workspace("T-0001");
    let to_implement = json!({
        "source": "human", "type": "state.overridden",
        "payload": { "to": "ready", "stage": "implement" },
    });
    let path = "/tasks/T-0001/events";
    assert_eq!(
        d.send(Method::POST, path, Some(TOKEN), &to_implement)
            .await
            .0,
        200
    );
    let mut first = d.open("cols=80&rows=24").await;
    first.until(|socket| socket.replayed()).await;
    let mut second = d.open("cols=80&rows=24&terminal=2").await;
    second.until(|socket| socket.replayed()).await;
    let shell = d.ptys()[1].clone();
    shell.set_program("node");
    shell.set_commands(&[
        "/home/me/.local/bin/agent --use-system-ca /home/me/.local/share/cursor-agent/1/index.js",
    ]);

    // Node mocks the lane. Here the task is at Implement of the default pipeline, whose group
    // gives Terminal 2 to Test spec: a lane no hook has set is done.
    let (_, listed) = d
        .call(Method::GET, "/tasks/T-0001/terminals", Some(TOKEN))
        .await;

    assert_eq!(
        listed["lanes"]["2"],
        json!({ "stage": "test-spec", "state": "done", "agent": "cursor" })
    );
    // The second half, a `beforeSubmitPrompt` hook that sets the lane running, is the next test:
    // it needs a shell that no stage uses, as Node's mock of `laneOfTerminal` cannot be set here.
}

// The by-hand shell state: a hook from Terminal 2, where no stage of the pipeline works, gives its
// agent a state of its own, which the list of the terminals shows (`terminals_of`).
#[tokio::test(flavor = "multi_thread")]
async fn shows_the_state_the_hooks_gave_an_agent_in_a_shell_no_stage_uses() {
    let d = bare("lane-hook").await;
    d.set_simple_pipeline();
    d.make_task("Fix the bug").await;
    d.give_workspace("T-0001");
    let mut first = d.open("cols=80&rows=24").await;
    first.until(|socket| socket.replayed()).await;
    let mut second = d.open("cols=80&rows=24&terminal=2").await;
    second.until(|socket| socket.replayed()).await;
    let shell = d.ptys()[1].clone();
    shell.set_program("node");
    shell.set_commands(&[
        "/home/me/.local/bin/agent --use-system-ca /home/me/.local/share/cursor-agent/1/index.js",
    ]);

    let (status, _) = d
        .hook(
            "cursor",
            "",
            &[("x-natamux-task", "T-0001"), ("x-natamux-terminal", "2")],
            Some(TOKEN),
            &json!({ "hook_event_name": "beforeSubmitPrompt" }),
        )
        .await;

    assert_eq!(status, 200);
    let (_, listed) = d
        .call(Method::GET, "/tasks/T-0001/terminals", Some(TOKEN))
        .await;
    assert_eq!(listed["lanes"]["2"]["state"], "running");
    assert_eq!(listed["lanes"]["2"]["agent"], "cursor");
}

async fn message(d: &TestDaemon, text: &str) -> (u16, serde_json::Value) {
    let body = json!({ "text": text });
    d.send(
        Method::POST,
        "/tasks/T-0001/agent/message",
        Some(TOKEN),
        &body,
    )
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn sends_a_review_comment_to_the_agent_in_the_first_terminal() {
    let d = daemon("comment", true).await;
    assert_eq!(message(&d, "Rename the function").await.0, 409);

    let mut socket = d.open("cols=80&rows=24").await;
    socket.until(|socket| socket.replayed()).await;
    // Only a shell runs: the comment would run as commands there.
    assert_eq!(message(&d, "Rename the function").await.0, 409);
    assert!(d.ptys()[0].written().is_empty());

    d.ptys()[0].set_program("node");
    let (status, _) = message(&d, "Rename the function").await;

    assert_eq!(status, 200);
    let pty = d.ptys()[0].clone();
    eventually(|| pty.written() == ["\x1b[200~Rename the function\x1b[201~", "\r"]).await;
    let (_, events) = d
        .call(Method::GET, "/tasks/T-0001/events", Some(TOKEN))
        .await;
    let last = &events["events"].as_array().unwrap().last().unwrap()["event"];
    assert_eq!(last["type"], "review.commented");
    assert_eq!(last["payload"]["text"], "Rename the function");
}

async fn with_workspace(name: &str) -> TestDaemon {
    let d = bare(name).await;
    d.make_task("Fix the bug").await;
    d.give_workspace("T-0001");
    d
}

async fn start(d: &TestDaemon, body: serde_json::Value) -> (u16, serde_json::Value) {
    d.send(Method::POST, "/tasks/T-0001/agent", Some(TOKEN), &body)
        .await
}

// Node's server tests run on a fixed platform (linux); the Rust test daemon runs the real one,
// and on Windows a launch is typed in full, with no script.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn types_the_agent_command_into_the_task_terminal() {
    let d = with_workspace("start").await;

    let (status, body) = start(&d, json!({ "agent": "claude-code" })).await;

    assert_eq!(status, 200, "{body}");
    assert_eq!(body["task"]["agent"], "claude-code");
    assert!(d.ptys()[0].written()[0].contains("agent-launch-"));
}

#[tokio::test(flavor = "multi_thread")]
async fn answers_404_for_an_agent_it_cannot_start_and_makes_a_missing_workspace() {
    let d = git_daemon("start-404").await;
    d.make_task("Fix the bug").await;

    let (status, _) = start(&d, json!({ "agent": "nope" })).await;

    assert_eq!(status, 404);
    let (_, task) = d.call(Method::GET, "/tasks/T-0001", Some(TOKEN)).await;
    assert!(task["task"]["workspace"].is_null());
    assert!(d.ptys().is_empty());

    let (status, started) = start(&d, json!({ "agent": "claude-code" })).await;

    assert_eq!(status, 200, "{started}");
    assert!(!started["task"]["workspace"].is_null());
    // Windows types the launch in full, with no script.
    if cfg!(unix) {
        assert!(d.ptys()[0].written().join("").contains("agent-launch-"));
    }
}

// T-0001 is done; Claude Code ended in Terminal 1 on session s-1.
async fn done_task(name: &str) -> TestDaemon {
    let d = with_workspace(name).await;
    let body =
        json!({ "source": "human", "type": "state.overridden", "payload": { "to": "done" } });
    d.send(Method::POST, "/tasks/T-0001/events", Some(TOKEN), &body)
        .await;
    let shell = EndedShell {
        name: "1".into(),
        agent: Some("claude-code".into()),
        stage: None,
        session: Some("s-1".into()),
        ended_at: "2026-10-20T00:00:00.000Z".into(),
        text: "Done.".into(),
    };
    save_ended_shells(&d.root.join("data"), "T-0001", &[shell]).unwrap();
    d
}

// Node's server tests run on a fixed platform (linux); the Rust test daemon runs the real one,
// and on Windows a launch is typed in full, with no script.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn starts_the_agent_in_terminal_1_on_its_session() {
    let d = done_task("reopen").await;

    let (status, body) = d.call(Method::POST, REOPEN, Some(TOKEN)).await;

    assert_eq!(status, 200, "{body}");
    assert_eq!(body["task"]["id"], "T-0001");
    assert_eq!(body["task"]["state"], "done");
    let typed = d.ptys().last().unwrap().written().join("");
    assert!(typed.contains("agent-launch-T-0001.sh"), "{typed}");
    let script = std::fs::read_to_string(d.root.join("data/agent-launch-T-0001.sh")).unwrap();
    assert!(script.contains("--resume 's-1'"), "{script}");

    // Its hooks leave the task done.
    let headers = [("x-natamux-task", "T-0001"), ("x-natamux-terminal", "1")];
    for name in ["UserPromptSubmit", "PreToolUse", "Stop"] {
        let body = json!({ "hook_event_name": name, "session_id": "s-2" });
        let (status, _) = d
            .hook("claude-code", "", &headers, Some(TOKEN), &body)
            .await;
        assert_eq!(status, 200, "{name}");
    }
    let (_, task) = d.call(Method::GET, "/tasks/T-0001", Some(TOKEN)).await;
    assert_eq!(task["task"]["state"], "done");
}

#[tokio::test(flavor = "multi_thread")]
async fn answers_409_while_the_agent_runs_or_for_a_task_that_is_not_finished() {
    let d = done_task("reopen-409").await;
    d.call(Method::POST, REOPEN, Some(TOKEN)).await;
    d.ptys().last().unwrap().set_program("node");
    let typed = d.ptys().last().unwrap().written().len();

    assert_eq!(d.call(Method::POST, REOPEN, Some(TOKEN)).await.0, 409);
    assert_eq!(d.ptys().last().unwrap().written().len(), typed);

    d.make_task("Fix another bug").await;
    let another = "/tasks/T-0002/agent/reopen";
    assert_eq!(d.call(Method::POST, another, Some(TOKEN)).await.0, 409);
}

#[tokio::test(flavor = "multi_thread")]
async fn answers_404_for_a_task_that_is_gone() {
    let d = bare("reopen-404").await;

    let (status, _) = d
        .call(Method::POST, "/tasks/T-0042/agent/reopen", Some(TOKEN))
        .await;

    assert_eq!(status, 404);
    assert!(d.ptys().is_empty());
}
