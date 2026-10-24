//! 28 `marks`, 29 `stage_files`, 32 `agent_token`.

use std::time::{Duration, Instant};

use hyper::Method;
use natamux_contract::daemon::{Body, Daemon, Token};
use natamux_contract::transcript::Transcript;
use serde_json::{Value, json};

use crate::{
    agent_token, both, call, cancel, event, get, kept, launch_script, make_twins, put,
    stage_events, start, task, wait_event, wait_file,
};

async fn mark(d: &Daemon, id: &str, which: &str, stage: &str, on: bool) -> Value {
    let body = json!({ "stage": stage, "on": on });
    kept(&put(d, &format!("/tasks/{id}/{which}"), body).await)
}

/// The workflow, CI and skip marks, and their refusals; at the group, the marked lane is skipped
/// as planned and Implement gets the workflow line.
#[tokio::test]
#[ignore = "needs both daemons: the daemon-contract CI job"]
async fn marks() {
    make_twins(&[2801], "[no-done] Marks", "").await;
    both(
        "marks",
        2801,
        &[],
        |d: Daemon, id, mut t: Transcript| async move {
            for (step, which, stage, on) in [
                ("workflow on", "workflow", "implement", true),
                ("workflow on a manual stage", "workflow", "review", true),
                ("ci on", "ci", "implement", true),
                ("ci off", "ci", "implement", false),
                ("ci on a review", "ci", "review", true),
                ("skip notes", "skip", "notes", true),
                ("skip the current stage", "skip", "spec", true),
                ("skip an unknown stage", "skip", "nope", true),
            ] {
                t.note(step, mark(&d, &id, which, stage, on).await);
            }
            let body = json!({ "stage": "", "on": "yes" });
            t.note(
                "bad body",
                kept(&put(&d, &format!("/tasks/{id}/skip"), body).await),
            );
            let moved = event(
                &d,
                &id,
                "state.overridden",
                json!({ "to": "ready", "stage": "build" }),
            );
            t.note("to the group", moved.await);
            wait_event(&d, &id, "stage.started", "implement", 1).await;
            // The lanes of the group start one after the other; the fast fake agent made that
            // visible. Wait until the skipped lane left `ready` (or a deadline, then compare).
            let deadline = Instant::now() + Duration::from_secs(30);
            while task(&d, &id).await["lanes"]["notes"] == "ready" && Instant::now() < deadline {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            t.note("at the group", task(&d, &id).await);
            t.note("implement launch", launch_script(&d, &id, "1").await);
            t.note("events", stage_events(&d, &id).await);
            cancel(&d, &id).await;
            t
        },
    )
    .await;
}

/// The stage files of a task: written, listed, read, refused; archived with the task when its
/// stage passes.
#[tokio::test]
#[ignore = "needs both daemons: the daemon-contract CI job"]
async fn stage_files() {
    make_twins(&[2901], "[no-done] Files", "").await;
    both(
        "stage_files",
        2901,
        &[],
        |d: Daemon, id, mut t: Transcript| async move {
            let files = format!("/tasks/{id}/files");
            let text = json!({ "text": "# Spec\n\n## Goal\n" });
            t.note(
                "write",
                kept(&put(&d, &format!("{files}/spec.md"), text).await),
            );
            t.note("list", names(&get(&d, &files).await.body));
            t.note("read", kept(&get(&d, &format!("{files}/spec.md")).await));
            t.note("missing", kept(&get(&d, &format!("{files}/plan.md")).await));
            t.note(
                "not markdown",
                kept(&get(&d, &format!("{files}/notes.txt")).await),
            );
            let dots = json!({ "text": "x" });
            t.note(
                "bad name",
                kept(&put(&d, &format!("{files}/a..md"), dots).await),
            );
            let big = json!({ "text": "x".repeat(1_000_001) });
            t.note(
                "too large",
                kept(&put(&d, &format!("{files}/big.md"), big).await),
            );
            t.note(
                "no text",
                kept(&put(&d, &format!("{files}/spec.md"), json!({})).await),
            );
            start(&d, &id).await;
            wait_event(&d, &id, "stage.started", "spec", 1).await;
            // The fast fake agent may be writing the spec: wait until it is whole, on both daemons.
            wait_file(&d, &id, "spec.md", "## Scope").await;
            let passed = kept(
                &call(
                    &d,
                    Method::POST,
                    &format!("/tasks/{id}/stage"),
                    Body::Json(json!({ "action": "pass", "stage": "spec" })),
                )
                .await,
            );
            t.note("spec passed", passed);
            let archive = format!("{}/tasks/{id}", d.data_dir());
            t.note("archive", json!(d.exec(&format!("ls '{archive}'")).await));
            cancel(&d, &id).await;
            t
        },
    )
    .await;
}

/// The names and sizes of a file list (times differ).
fn names(body: &Value) -> Value {
    let files = body["files"].as_array().cloned().unwrap_or_default();
    Value::Array(
        files
            .iter()
            .map(|file| json!([file["name"], file["size"]]))
            .collect(),
    )
}

/// What the agent token may and may not do to stages.
#[tokio::test]
#[ignore = "needs both daemons: the daemon-contract CI job"]
async fn agent_token_scenario() {
    make_twins(&[3201], "[no-done] Agent token", "").await;
    both(
        "agent_token",
        3201,
        &[],
        |d: Daemon, id, mut t: Transcript| async move {
            let token = agent_token(&d).await;
            let as_agent = |path: String, method: Method, body: Value| {
                let (d, token) = (d.clone(), token.clone());
                async move {
                    kept(
                        &d.request(method, &path, Body::Json(body), Token::Given(&token))
                            .await,
                    )
                }
            };
            start(&d, &id).await;
            wait_event(&d, &id, "stage.started", "spec", 1).await;
            // The fast fake agent may be writing the spec: wait until it is whole, on both daemons.
            wait_file(&d, &id, "spec.md", "## Scope").await;
            let stage = format!("/tasks/{id}/stage");
            for action in ["approve", "skip", "pass"] {
                let body = json!({ "action": action, "stage": "spec" });
                t.note(
                    &format!("agent {action}"),
                    as_agent(stage.clone(), Method::POST, body).await,
                );
            }
            for which in ["workflow", "ci", "skip"] {
                let body = json!({ "stage": "implement", "on": true });
                let path = format!("/tasks/{id}/{which}");
                t.note(
                    &format!("agent {which}"),
                    as_agent(path, Method::PUT, body).await,
                );
            }
            let report = json!({ "action": "report", "stage": "spec" });
            t.note(
                "agent report",
                as_agent(stage.clone(), Method::POST, report).await,
            );
            t.note("events", stage_events(&d, &id).await);
            cancel(&d, &id).await;
            t
        },
    )
    .await;
}
