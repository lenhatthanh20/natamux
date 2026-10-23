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

// Node asks the stage engine for the path of each event; the Rust daemon has none until part 3,
// so an event that would enter a branch is refused with 409 and nothing is logged.
#[tokio::test(flavor = "multi_thread")]
async fn refuses_an_event_that_enters_a_branch_with_409() {
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
    d.send(
        Method::POST,
        "/tasks",
        Some(TOKEN),
        &json!({ "title": "Example" }),
    )
    .await;
    for kind in ["task.readied", "agent.started"] {
        post(
            &d,
            TOKEN,
            "T-0001",
            json!({ "source": "human", "type": kind }),
        )
        .await;
    }
    let before = logged(&d).await;

    let (status, body) = post(
        &d,
        TOKEN,
        "T-0001",
        json!({ "source": "agent", "type": "stage.checked" }),
    )
    .await;

    assert_eq!(status, 409);
    assert_eq!(body["error"], "conflict");
    assert_eq!(logged(&d).await, before);
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
