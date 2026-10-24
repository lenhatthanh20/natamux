//! 33 `hook_turn_end`, 34 `hook_question`, 35 `stopped_early`, 36 `session_resume`,
//! 37 `hooks_route`; 38 `usage` and 39 `hook_time` are in `hooks_usage.rs`.
//!
//! The daemon images have no `natamux` command, so the scenarios post the hooks themselves, as
//! `natamux hook claude-code` does: the agent token, the task and terminal headers, and a body
//! like the one `scripts/fake-agent.sh` builds.

use std::time::{Duration, Instant};

use hyper::Method;
use natamux_contract::daemon::{Answer, Body, Daemon, Token};
use natamux_contract::events::Events;
use natamux_contract::socket::Socket;
use natamux_contract::transcript::Transcript;
use serde_json::{Value, json};

use crate::{
    agent_token, both, cancel, get, kept, launch_script, make_twins, on_task, report, stage_events,
    start, task, wait_at, wait_event, wait_file, wait_value,
};

#[path = "hooks_usage.rs"]
mod usage_and_time;

/// One agent of a twin task: what it needs to post hooks.
pub struct Agent {
    pub d: Daemon,
    pub id: String,
    pub token: String,
    /// The task's workspace, the `cwd` of the hooks.
    pub cwd: String,
}

impl Agent {
    pub async fn new(d: &Daemon, id: &str) -> Agent {
        let body = get(d, &format!("/tasks/{id}")).await.body;
        Agent {
            d: d.clone(),
            id: id.to_string(),
            token: agent_token(d).await,
            cwd: body["task"]["workspace"]
                .as_str()
                .unwrap_or_default()
                .into(),
        }
    }

    /// A hook to `path` (`/hooks/cursor?...`), with the task header and, when given, the
    /// terminal header.
    pub async fn raw(&self, path: &str, terminal: Option<&str>, body: Value) -> Answer {
        let mut headers = vec![("x-natamux-task", self.id.as_str())];
        if let Some(terminal) = terminal {
            headers.push(("x-natamux-terminal", terminal));
        }
        self.d
            .request_with(
                Method::POST,
                path,
                Body::Json(body),
                Token::Given(&self.token),
                &headers,
            )
            .await
    }

    /// A Claude Code hook from Terminal 1, `extra` merged into its body. Gives status and body.
    pub async fn claude(&self, event: &str, extra: Value) -> Value {
        let mut body = json!({ "hook_event_name": event, "cwd": self.cwd });
        if let (Some(body), Some(extra)) = (body.as_object_mut(), extra.as_object()) {
            body.extend(extra.clone());
        }
        kept(&self.raw("/hooks/claude-code", Some("1"), body).await)
    }

    /// The tool line of a `PreToolUse` hook, a file of the workspace.
    pub async fn edit(&self) -> Value {
        let file = format!("{}/src/a.ts", self.cwd);
        let tool = json!({ "tool_name": "Edit", "tool_input": { "file_path": file } });
        self.claude("PreToolUse", tool).await
    }
}

/// A task at its first stage with the agent running (no hook has come yet).
async fn running(d: &Daemon, id: &str) {
    start(d, id).await;
    wait_event(d, id, "stage.started", "spec", 1).await;
    wait_at(d, id, "spec", "running").await;
}

/// What the dashboards were told about the task's activity, in order.
fn activity_changes(events: &Events, id: &str) -> Vec<Value> {
    events
        .messages
        .iter()
        .filter(|message| message["kind"] == "activity.changed" && message["taskId"] == id)
        .map(|message| crate::clean(message["activity"].clone()))
        .collect()
}

/// `GET /activity` for the task.
async fn activity(d: &Daemon, id: &str) -> Value {
    crate::clean(get(d, "/activity").await.body["activity"][id].clone())
}

/// On a running agent stage: a prompt, a tool, the end of a turn (needs-input), a new prompt
/// (resumed), and an end of turn with background work (still running, "Waiting for ...").
#[tokio::test]
#[ignore = "needs both daemons: the daemon-contract CI job"]
async fn hook_turn_end() {
    make_twins(&[3301], "[no-done] Turn end", "").await;
    both(
        "hook_turn_end",
        3301,
        &[],
        |d: Daemon, id, mut t: Transcript| async move {
            running(&d, &id).await;
            let agent = Agent::new(&d, &id).await;
            let mut events = Events::open(&d, Some(&d.token)).await;
            t.note("prompt", agent.claude("UserPromptSubmit", json!({})).await);
            t.note("activity after the prompt", activity(&d, &id).await);
            t.note("tool", agent.edit().await);
            t.note("activity after the tool", activity(&d, &id).await);
            t.note("turn end", agent.claude("Stop", json!({})).await);
            wait_at(&d, &id, "spec", "needs-input").await;
            t.note("at the turn end", task(&d, &id).await);
            t.note("activity after the turn end", activity(&d, &id).await);
            t.note(
                "new prompt",
                agent.claude("UserPromptSubmit", json!({})).await,
            );
            wait_at(&d, &id, "spec", "running").await;
            t.note("after the new prompt", task(&d, &id).await);
            let work = json!({
                "background_tasks": [
                    { "type": "shell", "status": "running", "command": "pnpm test" },
                    { "type": "shell", "status": "completed", "command": "done" },
                    { "type": "monitor", "description": "watch" }
                ],
                "session_crons": [{ "prompt": "check" }],
            });
            t.note("turn end with work", agent.claude("Stop", work).await);
            t.note("while it waits", task(&d, &id).await);
            t.note("activity while it waits", activity(&d, &id).await);
            t.note("last turn end", agent.claude("Stop", json!({})).await);
            wait_at(&d, &id, "spec", "needs-input").await;
            // Prompt, tool, turn end, prompt, turn end with work, turn end: six changes.
            events
                .until(|messages| {
                    messages
                        .iter()
                        .filter(|m| m["kind"] == "activity.changed" && m["taskId"] == id)
                        .count()
                        >= 6
                })
                .await;
            t.note(
                "activity pushes",
                Value::Array(activity_changes(&events, &id)),
            );
            t.note("events", stage_events(&d, &id).await);
            cancel(&d, &id).await;
            t
        },
    )
    .await;
}

/// `permission_prompt` and `elicitation_dialog` notifications and `PermissionRequest` ask; other
/// notification types change nothing; a tool that ran, or a prompt, answers.
#[tokio::test]
#[ignore = "needs both daemons: the daemon-contract CI job"]
async fn hook_question() {
    make_twins(&[3401], "[no-done] Question", "").await;
    both(
        "hook_question",
        3401,
        &[],
        |d: Daemon, id, mut t: Transcript| async move {
            running(&d, &id).await;
            let agent = Agent::new(&d, &id).await;
            let note = |kind: &str| json!({ "notification_type": kind, "message": "Allow Bash?" });
            t.note(
                "idle",
                agent.claude("Notification", note("idle_prompt")).await,
            );
            t.note("after idle", task(&d, &id).await);
            t.note(
                "asks",
                agent
                    .claude("Notification", note("permission_prompt"))
                    .await,
            );
            wait_at(&d, &id, "spec", "needs-input").await;
            t.note("after asks", task(&d, &id).await);
            t.note("tool ran", agent.claude("PostToolUse", json!({})).await);
            wait_at(&d, &id, "spec", "running").await;
            t.note("after tool", task(&d, &id).await);
            t.note(
                "permission request",
                agent.claude("PermissionRequest", json!({})).await,
            );
            wait_at(&d, &id, "spec", "needs-input").await;
            t.note("after request", task(&d, &id).await);
            t.note("prompt", agent.claude("UserPromptSubmit", json!({})).await);
            wait_at(&d, &id, "spec", "running").await;
            t.note(
                "dialog",
                agent
                    .claude("Notification", note("elicitation_dialog"))
                    .await,
            );
            wait_at(&d, &id, "spec", "needs-input").await;
            t.note("after dialog", task(&d, &id).await);
            t.note(
                "ask twice",
                agent.claude("PermissionRequest", json!({})).await,
            );
            t.note("after twice", task(&d, &id).await);
            t.note("events", stage_events(&d, &id).await);
            cancel(&d, &id).await;
            t
        },
    )
    .await;
}

/// A turn end with no report is "stopped early" (the stage waits for input, the agent can go
/// on). After a report, the turn end that follows applies nothing and clears the activity line.
#[tokio::test]
#[ignore = "needs both daemons: the daemon-contract CI job"]
async fn stopped_early() {
    make_twins(&[3501], "[no-done] Stopped early", "").await;
    both(
        "stopped_early",
        3501,
        &[],
        |d: Daemon, id, mut t: Transcript| async move {
            running(&d, &id).await;
            let agent = Agent::new(&d, &id).await;
            wait_file(&d, &id, "spec.md", "## Scope").await;
            t.note("early stop", agent.claude("Stop", json!({})).await);
            wait_at(&d, &id, "spec", "needs-input").await;
            t.note("stopped early", task(&d, &id).await);
            t.note("go on", agent.claude("UserPromptSubmit", json!({})).await);
            wait_at(&d, &id, "spec", "running").await;
            t.note("report", report(&d, &id, "spec", "1").await);
            wait_at(&d, &id, "spec", "review").await;
            t.note("tool after the report", agent.edit().await);
            t.note("activity before", activity(&d, &id).await);
            t.note("after the report", task(&d, &id).await);
            t.note("turn end", agent.claude("Stop", json!({})).await);
            t.note("nothing applied", task(&d, &id).await);
            t.note("activity after", activity(&d, &id).await);
            t.note("events", stage_events(&d, &id).await);
            cancel(&d, &id).await;
            t
        },
    )
    .await;
}

/// A hook that names a session from Terminal 1 is kept in the open-shells file; after a restart
/// the agent starts again on that session.
#[tokio::test]
#[ignore = "needs both daemons: the daemon-contract CI job"]
async fn session_resume() {
    make_twins(&[3601], "[no-done] Session", "").await;
    both(
        "session_resume",
        3601,
        &[],
        |d: Daemon, id, mut t: Transcript| async move {
            running(&d, &id).await;
            let agent = Agent::new(&d, &id).await;
            let session = json!({ "session_id": "sess-3601-abc" });
            t.note("prompt", agent.claude("UserPromptSubmit", session).await);
            let file = format!("{}/tasks/{id}.open-shells.json", d.data_dir());
            let deadline = Instant::now() + Duration::from_secs(30);
            while !d
                .exec_in(&format!("cat '{file}' 2>/dev/null"))
                .await
                .contains("sess-3601-abc")
            {
                assert!(Instant::now() < deadline, "{}: session not kept", d.name);
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
            d.restart().await;
            wait_value(&d, &format!("/tasks/{id}/events"), |body| {
                let launches = body["events"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|entry| entry["event"]["type"] == "agent.launched")
                    .count();
                (launches >= 2).then_some(())
            })
            .await;
            let script = launch_script(&d, &id, "1").await;
            let text = script.as_str().unwrap_or_default();
            assert!(
                text.contains("--resume") && text.contains("sess-3601-abc"),
                "{text}"
            );
            t.note("relaunch", script);
            t.note("after the restart", task(&d, &id).await);
            t.note("events", stage_events(&d, &id).await);
            cancel(&d, &id).await;
            t
        },
    )
    .await;
}

/// The refusals and the ways a hook finds its task: no token 401, unknown agent 404, no event
/// name 400, no task 200 `{}`, a task by `cwd`, Copilot's `?event=`, `StopFailure`, and a hook
/// from a shell started by hand (its state shows in the terminals list, the task is left).
#[tokio::test]
#[ignore = "needs both daemons: the daemon-contract CI job"]
async fn hooks_route() {
    make_twins(&[3701], "[no-done] Hooks route", "").await;
    both(
        "hooks_route",
        3701,
        &[],
        |d: Daemon, id, mut t: Transcript| async move {
            running(&d, &id).await;
            let agent = Agent::new(&d, &id).await;
            let stop = json!({ "hook_event_name": "Stop" });
            let no_token = d
                .request(
                    Method::POST,
                    "/hooks/claude-code",
                    Body::Json(stop),
                    Token::None,
                )
                .await;
            t.note("no token", kept(&no_token));
            let nope = agent
                .raw("/hooks/nope", None, json!({ "hook_event_name": "Stop" }))
                .await;
            t.note("unknown agent", kept(&nope));
            let nameless = agent
                .raw("/hooks/claude-code", None, json!({ "cwd": "/x" }))
                .await;
            t.note("no event name", kept(&nameless));
            let elsewhere = json!({ "hook_event_name": "Stop", "cwd": "/elsewhere" });
            t.note("no task", bare(&agent, elsewhere).await);
            t.note("after no task", task(&d, &id).await);
            let unused = json!({ "hook_event_name": "SessionEnd", "cwd": agent.cwd });
            t.note("unused hook", bare(&agent, unused).await);
            let by_cwd = json!({ "hook_event_name": "Stop", "cwd": format!("{}/src", agent.cwd) });
            t.note("task by cwd", bare(&agent, by_cwd).await);
            wait_at(&d, &id, "spec", "needs-input").await;
            t.note("after cwd", task(&d, &id).await);
            t.note(
                "copilot prompt",
                copilot(&agent, "userPromptSubmitted").await,
            );
            wait_at(&d, &id, "spec", "running").await;
            t.note("after copilot prompt", task(&d, &id).await);
            t.note("copilot stop", copilot(&agent, "agentStop").await);
            wait_at(&d, &id, "spec", "needs-input").await;
            t.note("after copilot stop", task(&d, &id).await);
            t.note("prompt", agent.claude("UserPromptSubmit", json!({})).await);
            t.note("failure", agent.claude("StopFailure", json!({})).await);
            wait_at(&d, &id, "spec", "failed").await;
            t.note("after failure", task(&d, &id).await);
            t.note("tool", agent.edit().await);
            wait_at(&d, &id, "spec", "running").await;
            t.note("after tool", task(&d, &id).await);
            by_hand(&d, &id, &agent, &mut t).await;
            t.note("events", stage_events(&d, &id).await);
            cancel(&d, &id).await;
            t
        },
    )
    .await;
}

/// A hook with no task header and no terminal: the daemon finds the task by `cwd`.
async fn bare(agent: &Agent, body: Value) -> Value {
    let hook = agent.d.request(
        Method::POST,
        "/hooks/claude-code",
        Body::Json(body),
        Token::Given(&agent.token),
    );
    kept(&hook.await)
}

/// A Copilot hook of Terminal 1: its event name comes in the query.
async fn copilot(agent: &Agent, event: &str) -> Value {
    let path = format!("/hooks/copilot?event={event}");
    kept(&agent.raw(&path, Some("1"), json!({})).await)
}

/// A program runs in Terminal 2, which no stage uses: Cursor's hooks give it a state of its own
/// in the terminals list and leave the task as it was.
async fn by_hand(d: &Daemon, id: &str, agent: &Agent, t: &mut Transcript) {
    let mut socket = Socket::open(&on_task(d, id), "terminal=2&cols=80&rows=24").await;
    socket.until(|socket| socket.count("replayed") > 0).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    socket.input("sleep 300\r").await;
    wait_value(d, &format!("/tasks/{id}/terminals"), |body| {
        let runs = body["agents"].as_array();
        runs.is_some_and(|names| names.iter().any(|name| name == "2"))
            .then_some(())
    })
    .await;
    let lane = |body: &Value| json!([body["agents"], body["lanes"]["2"]]);
    let before = task(d, id).await;
    for (step, event, extra) in [
        ("cursor prompt", "beforeSubmitPrompt", json!({})),
        ("cursor stop", "stop", json!({ "status": "completed" })),
    ] {
        let mut body = json!({ "hook_event_name": event });
        if let (Some(map), Some(extra)) = (body.as_object_mut(), extra.as_object()) {
            map.extend(extra.clone());
        }
        t.note(
            step,
            kept(&agent.raw("/hooks/cursor", Some("2"), body).await),
        );
        let listed = get(d, &format!("/tasks/{id}/terminals")).await;
        t.note(&format!("{step}: terminals"), lane(&listed.body));
    }
    t.note("task left as it was", json!(task(d, id).await == before));
    drop(socket);
}
