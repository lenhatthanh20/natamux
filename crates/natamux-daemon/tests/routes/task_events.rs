//! The `task events` group and the gate-event cases of `the agent token of the task shells`.

use hyper::Method;
use natamux_core::events::schema::GATE_EVENT_TYPES;
use serde_json::{Value, json};

use crate::server::{AGENT_TOKEN, TOKEN, TestDaemon, bare};

async fn task() -> TestDaemon {
    let d = bare("events").await;
    let created = d
        .send(
            Method::POST,
            "/tasks",
            Some(TOKEN),
            &json!({ "title": "Example" }),
        )
        .await;
    assert_eq!(created.0, 201);
    d
}

async fn post(d: &TestDaemon, token: &str, id: &str, body: Value) -> (u16, Value) {
    d.send(
        Method::POST,
        &format!("/tasks/{id}/events"),
        Some(token),
        &body,
    )
    .await
}

/// The (type, source) of each logged event.
async fn logged(d: &TestDaemon) -> Vec<(String, String)> {
    let (_, body) = d
        .call(Method::GET, "/tasks/T-0001/events", Some(TOKEN))
        .await;
    body["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| {
            let event = &entry["event"];
            (
                event["type"].as_str().unwrap().to_string(),
                event["source"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

fn entry(kind: &str, source: &str) -> (String, String) {
    (kind.to_string(), source.to_string())
}

#[tokio::test(flavor = "multi_thread")]
async fn applies_an_event_and_filters_tasks_by_the_new_state() {
    let d = task().await;

    let (status, applied) = post(
        &d,
        TOKEN,
        "T-0001",
        json!({ "source": "human", "type": "task.readied" }),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(applied["accepted"], true);
    assert_eq!(applied["task"]["state"], "ready");

    let (_, ready) = d.call(Method::GET, "/tasks?state=ready", Some(TOKEN)).await;
    assert_eq!(ready["tasks"].as_array().unwrap().len(), 1);
}

/// The payload of each logged event of a task.
async fn payloads(d: &TestDaemon, id: &str) -> Vec<(String, Value)> {
    let (_, body) = d
        .call(Method::GET, &format!("/tasks/{id}/events"), Some(TOKEN))
        .await;
    body["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| {
            let event = &entry["event"];
            (
                event["type"].as_str().unwrap().to_string(),
                event["payload"].clone(),
            )
        })
        .collect()
}

// Any event may end the step before a branch, so each goes through the conditions. Node spies on
// `withPath`; this checks what it does: the logged event keeps the path and the reason.
#[tokio::test(flavor = "multi_thread")]
async fn checks_the_conditions_of_a_branch_before_any_event_of_a_person_or_an_agent() {
    let d = bare("branch").await;
    let pipeline = json!({ "stages": [
        { "id": "spec", "kind": "agent", "gate": "auto" },
        { "id": "kind", "paths": [
            { "id": "bug", "when": { "type": "command", "run": "true" },
              "stages": [{ "id": "repro", "kind": "agent", "gate": "auto" }] },
            { "id": "feature", "stages": [{ "id": "plan", "kind": "agent", "gate": "auto" }] },
        ] },
    ] });
    std::fs::write(d.root.join("repo/.natamux.json"), pipeline.to_string()).unwrap();
    for (n, (token, source)) in [(TOKEN, "human"), (AGENT_TOKEN, "agent")]
        .into_iter()
        .enumerate()
    {
        let created = d
            .send(
                Method::POST,
                "/tasks",
                Some(TOKEN),
                &json!({ "title": "Example" }),
            )
            .await;
        assert_eq!(created.0, 201);
        let id = format!("T-000{}", n + 1);
        d.give_workspace(&id);
        for kind in ["task.readied", "agent.started"] {
            post(&d, TOKEN, &id, json!({ "source": "human", "type": kind })).await;
        }

        let (status, applied) = post(
            &d,
            token,
            &id,
            json!({ "source": "human", "type": "stage.checked" }),
        )
        .await;

        assert_eq!(status, 200, "{source}: {applied}");
        let logged = payloads(&d, &id).await;
        let (kind, payload) = logged.last().unwrap();
        assert_eq!(kind, "stage.checked", "{source}");
        assert_eq!(payload["path"], "bug", "{source}: {payload}");
        assert!(payload["because"].is_string(), "{source}: {payload}");
        // An event that enters no branch has no path and no reason.
        let readied = logged.iter().find(|(kind, _)| kind == "task.readied");
        let readied = &readied.unwrap().1;
        assert!(
            readied.get("path").is_none() && readied.get("because").is_none(),
            "{source}"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn reports_an_event_that_does_not_fit_as_not_accepted() {
    let d = task().await;
    let (status, applied) = post(
        &d,
        TOKEN,
        "T-0001",
        json!({ "source": "agent", "type": "agent.finished" }),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(applied["accepted"], false);
    assert_eq!(applied["task"]["state"], "draft");
}

#[tokio::test(flavor = "multi_thread")]
async fn lists_the_events_of_a_task() {
    let d = task().await;
    post(
        &d,
        TOKEN,
        "T-0001",
        json!({ "source": "human", "type": "task.readied" }),
    )
    .await;

    let types: Vec<String> = logged(&d).await.into_iter().map(|(kind, _)| kind).collect();

    assert_eq!(types, ["task.created", "task.readied"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn rejects_an_unknown_event_type_with_400() {
    let d = task().await;
    let (status, _) = post(
        &d,
        TOKEN,
        "T-0001",
        json!({ "source": "agent", "type": "agent.exploded" }),
    )
    .await;
    assert_eq!(status, 400);
}

#[tokio::test(flavor = "multi_thread")]
async fn returns_404_for_events_on_an_unknown_task() {
    let d = bare("events-404").await;
    let (status, _) = post(
        &d,
        TOKEN,
        "T-0099",
        json!({ "source": "human", "type": "task.readied" }),
    )
    .await;
    assert_eq!(status, 404);
}

#[tokio::test(flavor = "multi_thread")]
async fn does_not_let_a_client_send_an_event_only_the_daemon_makes() {
    for kind in [
        "task.created",
        "workspace.created",
        "workspace.removed",
        "agent.active",
    ] {
        let d = task().await;
        let (status, _) = post(
            &d,
            TOKEN,
            "T-0001",
            json!({ "source": "human", "type": kind }),
        )
        .await;
        assert_eq!(status, 400, "{kind}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn accepts_the_agent_token_for_what_an_agent_does() {
    let d = task().await;
    let (status, body) = d
        .call(Method::GET, "/tasks/T-0001", Some(AGENT_TOKEN))
        .await;
    assert_eq!(status, 200);
    assert_eq!(body["task"]["id"], "T-0001");
}

#[tokio::test(flavor = "multi_thread")]
async fn records_what_the_agent_token_sends_as_the_agent_whatever_source_it_names() {
    let d = task().await;
    let (status, applied) = post(
        &d,
        AGENT_TOKEN,
        "T-0001",
        json!({ "source": "human", "type": "task.readied" }),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(applied["accepted"], true);
    assert_eq!(applied["task"]["state"], "ready");
    assert_eq!(
        logged(&d).await.last(),
        Some(&entry("task.readied", "agent"))
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_agent_token_may_not_send_a_gate_event() {
    for kind in GATE_EVENT_TYPES {
        let d = task().await;
        let before = logged(&d).await;
        let (status, body) = post(
            &d,
            AGENT_TOKEN,
            "T-0001",
            json!({ "source": "human", "type": kind, "payload": { "to": "done" } }),
        )
        .await;
        assert_eq!(status, 403, "{kind}");
        assert_eq!(body["error"], "forbidden", "{kind}");
        assert_eq!(logged(&d).await, before, "{kind}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn still_answers_401_to_a_wrong_token_on_a_route_the_agent_token_may_not_use() {
    let d = task().await;
    let put = d
        .send(
            Method::PUT,
            "/repos/default/settings",
            Some("nope"),
            &json!({ "agents": { "permissionMode": "ask" } }),
        )
        .await;
    assert_eq!(put.0, 401);
    let (status, _) = post(
        &d,
        "nope",
        "T-0001",
        json!({ "source": "human", "type": "task.readied" }),
    )
    .await;
    assert_eq!(status, 401);
    assert_eq!(logged(&d).await, [entry("task.created", "system")]);
}

// The activity half of 'sends the activity after a hook, and touches the workspace after a tool
// ran' (`pushed to the dashboards`). The other half, that `workspaces.touch` runs after a tool
// ran, is part 5: the hook service calls its `workspace_changed` callback (tested in
// `tests/hook_service/pushed.rs`), and the daemon has none yet. Node's `bus.subscribe` is the
// same here.
#[tokio::test(flavor = "multi_thread")]
async fn sends_the_activity_after_a_hook() {
    use natamux_daemon::shared::event_bus::DaemonMessage;
    use std::sync::{Arc, Mutex};

    let d = bare("events-activity").await;
    let sent: Arc<Mutex<Vec<DaemonMessage>>> = Arc::default();
    let sink = Arc::clone(&sent);
    d.daemon
        .state
        .bus
        .subscribe(move |message| sink.lock().unwrap().push(message.clone()));
    d.make_task("Fix the bug").await;
    let edit = json!({
        "hook_event_name": "PreToolUse",
        "tool_name": "Edit",
        "tool_input": { "file_path": "a.ts" },
    });

    let (status, _) = d
        .hook(
            "claude-code",
            "",
            &[("x-natamux-task", "T-0001")],
            Some(TOKEN),
            &edit,
        )
        .await;

    assert_eq!(status, 200);
    let activity = sent
        .lock()
        .unwrap()
        .iter()
        .find_map(|message| match message {
            DaemonMessage::ActivityChanged { task_id, activity } if task_id == "T-0001" => {
                Some(activity.clone())
            }
            _ => None,
        })
        .unwrap();
    assert_eq!(activity.len(), 1);
    assert_eq!(activity["1"]["text"], "Edit a.ts");
    assert!(activity["1"]["at"].is_string());
}
