//! 30 `by_hand_and_finish`, 31 `restart_and_resume`.

use std::time::Duration;

use natamux_contract::daemon::Daemon;
use natamux_contract::transcript::Transcript;
use serde_json::{Value, json};

use crate::{
    both, cancel, get, kept, launch_script, make_twins, post, stage_events, start, task, terminals,
    wait_event, wait_value,
};

/// The kept last screens: names, agents and stages (the screens hold the fake agent's random
/// pauses' output only as text, compared by its first line).
fn ended(body: &Value) -> Value {
    let shells = body["shells"].as_array().cloned().unwrap_or_default();
    Value::Array(
        shells
            .iter()
            .map(|shell| {
                json!([
                    shell["name"],
                    shell["agent"],
                    shell["stage"],
                    shell["session"]
                ])
            })
            .collect(),
    )
}

/// An agent started by a person on a new task starts its first stage; a message and a prompt go
/// to the running agent; a cancelled task ends its shells and keeps their last screens; the agent
/// of Terminal 1 comes back on a reopen.
#[tokio::test]
#[ignore = "needs both daemons: the daemon-contract CI job"]
async fn by_hand_and_finish() {
    make_twins(&[3001], "[no-done] By hand", "").await;
    both(
        "by_hand_and_finish",
        3001,
        &[],
        |d: Daemon, id, mut t: Transcript| async move {
            let agent = format!("/tasks/{id}/agent");
            t.note(
                "unknown agent",
                kept(&post(&d, &agent, json!({ "agent": "nope" })).await),
            );
            let message = json!({ "text": "Before any agent" });
            t.note(
                "message too early",
                kept(&post(&d, &format!("{agent}/message"), message).await),
            );
            t.note(
                "start",
                kept(&post(&d, &agent, json!({ "agent": "claude-code" })).await),
            );
            wait_event(&d, &id, "stage.started", "spec", 1).await;
            let message = json!({ "text": "Keep it short" });
            t.note(
                "message",
                kept(&post(&d, &format!("{agent}/message"), message).await),
            );
            let prompt = json!({ "agent": "claude-code", "prompt": "Go on" });
            t.note("prompt", kept(&post(&d, &agent, prompt).await));
            t.note(
                "reopen while running",
                kept(&post(&d, &format!("{agent}/reopen"), json!({})).await),
            );
            cancel(&d, &id).await;
            let shells = wait_value(&d, &format!("/tasks/{id}/ended-shells"), |body| {
                body["shells"]
                    .as_array()
                    .filter(|shells| !shells.is_empty())
                    .map(|_| body.clone())
            })
            .await;
            t.note("ended shells", ended(&shells));
            t.note(
                "reopen",
                kept(&post(&d, &format!("{agent}/reopen"), json!({})).await),
            );
            t.note(
                "reopen again",
                kept(&post(&d, &format!("{agent}/reopen"), json!({})).await),
            );
            t.note("reopen launch", launch_script(&d, &id, "1").await);
            t.note("events", stage_events(&d, &id).await);
            t
        },
    )
    .await;
}

/// A restart while the agent of a stage runs: its shell is held, and the agent starts again on
/// the newest session of the folder.
#[tokio::test]
#[ignore = "needs both daemons: the daemon-contract CI job"]
async fn restart_and_resume() {
    make_twins(&[3101], "[no-done] Restart", "").await;
    both(
        "restart_and_resume",
        3101,
        &[],
        |d: Daemon, id, mut t: Transcript| async move {
            start(&d, &id).await;
            wait_event(&d, &id, "stage.started", "spec", 1).await;
            // The list of open shells is written a moment after a change.
            tokio::time::sleep(Duration::from_secs(4)).await;
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
            t.note("after the restart", task(&d, &id).await);
            t.note("relaunch", launch_script(&d, &id, "1").await);
            tokio::time::sleep(Duration::from_secs(2)).await;
            t.note("terminals", terminals(&d, &id).await);
            t.note("events", stage_events(&d, &id).await);
            t.note(
                "ended shells",
                kept(&get(&d, &format!("/tasks/{id}/ended-shells")).await),
            );
            cancel(&d, &id).await;
            t
        },
    )
    .await;
}
