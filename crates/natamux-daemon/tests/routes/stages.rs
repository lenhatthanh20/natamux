//! The stage cases of `server.test.ts`: send back, the workflow and skip marks, and what the agent
//! token may do with a stage. The stage files have no case there.

use hyper::Method;
use serde_json::{Value, json};

use crate::server::{TOKEN, TestDaemon, bare};

async fn task() -> TestDaemon {
    let d = bare("stages").await;
    d.set_simple_pipeline();
    d.make_task("Example").await;
    d
}

async fn stage(d: &TestDaemon, token: &str, body: Value) -> (u16, Value) {
    d.send(Method::POST, "/tasks/T-0001/stage", Some(token), &body)
        .await
}

async fn put(d: &TestDaemon, token: &str, path: &str, body: Value) -> (u16, Value) {
    d.send(
        Method::PUT,
        &format!("/tasks/T-0001/{path}"),
        Some(token),
        &body,
    )
    .await
}

async fn send_event(d: &TestDaemon, kind: &str) {
    let body = json!({ "source": "human", "type": kind });
    d.send(Method::POST, "/tasks/T-0001/events", Some(TOKEN), &body)
        .await;
}

/// The task is ready and its agent started.
async fn running() -> TestDaemon {
    let d = task().await;
    send_event(&d, "task.readied").await;
    send_event(&d, "agent.started").await;
    d
}

fn entry(kind: &str, source: &str) -> (String, String) {
    (kind.to_string(), source.to_string())
}

#[tokio::test(flavor = "multi_thread")]
async fn answers_400_for_a_send_back_to_an_unknown_or_later_stage() {
    let d = task().await;
    for to in ["nope", "release"] {
        let (status, body) = stage(&d, TOKEN, json!({ "action": "reopen", "to": to })).await;
        assert_eq!(status, 400, "{to}");
        assert!(
            body["message"]
                .as_str()
                .unwrap()
                .contains(&format!("\"{to}\"")),
            "{body}"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn sets_the_workflow_of_a_stage_and_refuses_a_stage_that_does_not_offer_one() {
    let d = task().await;

    // The pipeline of these tests offers no workflow on any stage.
    let (status, body) = put(
        &d,
        TOKEN,
        "workflow",
        json!({ "stage": "review", "on": true }),
    )
    .await;
    assert_eq!(status, 400);
    assert!(
        body["message"]
            .as_str()
            .unwrap()
            .contains("project settings"),
        "{body}"
    );

    let (status, body) = put(
        &d,
        TOKEN,
        "workflow",
        json!({ "stage": "implement", "on": false }),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["task"]["id"], "T-0001");
    assert_eq!(body["task"]["workflowStages"], json!([]));

    let missing = put(&d, TOKEN, "workflow", json!({ "stage": "implement" })).await;
    assert_eq!(missing.0, 400);
    let unknown = d
        .send(
            Method::PUT,
            "/tasks/T-0099/workflow",
            Some(TOKEN),
            &json!({ "stage": "implement", "on": true }),
        )
        .await;
    assert_eq!(unknown.0, 404);
}

#[tokio::test(flavor = "multi_thread")]
async fn marks_a_later_stage_to_skip_and_refuses_the_current_one() {
    let d = task().await;

    let (status, body) = put(&d, TOKEN, "skip", json!({ "stage": "review", "on": true })).await;
    assert_eq!(status, 200);
    assert_eq!(body["task"]["id"], "T-0001");
    assert_eq!(body["task"]["skipStages"], json!(["review"]));

    let (status, body) = put(
        &d,
        TOKEN,
        "skip",
        json!({ "stage": "implement", "on": true }),
    )
    .await;
    assert_eq!(status, 400);
    assert!(body["message"].as_str().unwrap().contains("Skip"), "{body}");
    for bad in [
        json!({ "stage": "review" }),
        json!({ "stage": "review", "on": "yes" }),
        json!({ "stage": 3, "on": true }),
    ] {
        assert_eq!(put(&d, TOKEN, "skip", bad).await.0, 400);
    }
    let unknown = d
        .send(
            Method::PUT,
            "/tasks/T-0099/skip",
            Some(TOKEN),
            &json!({ "stage": "review", "on": true }),
        )
        .await;
    assert_eq!(unknown.0, 404);
}

#[tokio::test(flavor = "multi_thread")]
async fn may_not_approve_skip_or_pass_a_stage() {
    for action in ["approve", "skip", "pass"] {
        let d = task().await;
        let agent_token: &str = &d.agent_token("T-0001");
        let before = d.events_of("T-0001").await;

        let (status, body) = stage(&d, agent_token, json!({ "action": action })).await;

        assert_eq!(status, 403, "{action}");
        assert!(
            body["message"].as_str().unwrap().contains("Only a person"),
            "{body}"
        );
        assert_eq!(d.events_of("T-0001").await, before, "{action}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn records_a_stage_action_it_may_take_as_the_agent() {
    let d = running().await;
    let agent_token: &str = &d.agent_token("T-0001");

    let (status, _) = stage(
        &d,
        agent_token,
        json!({ "action": "fail", "reason": "Stuck" }),
    )
    .await;

    assert_eq!(status, 200);
    assert_eq!(
        d.events_of("T-0001").await.last(),
        Some(&entry("stage.failed", "agent"))
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn records_a_question_it_sends_as_a_person_as_the_agent() {
    let d = running().await;
    let agent_token: &str = &d.agent_token("T-0001");
    let body = json!({
        "source": "human", "type": "agent.asked", "payload": { "question": "Which file?" },
    });

    let (_, asked) = d
        .send(
            Method::POST,
            "/tasks/T-0001/events",
            Some(agent_token),
            &body,
        )
        .await;

    assert_eq!(asked["accepted"], true);
    assert_eq!(asked["task"]["state"], "needs-input");
    assert_eq!(
        d.events_of("T-0001").await.last(),
        Some(&entry("agent.asked", "agent"))
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn records_a_report_and_a_send_back_as_the_agent_and_a_start_never_as_a_person() {
    let d = running().await;
    let agent_token: &str = &d.agent_token("T-0001");
    stage(&d, agent_token, json!({ "action": "report" })).await;
    let reported: Vec<_> = d
        .events_of("T-0001")
        .await
        .into_iter()
        .filter(|(kind, _)| kind == "stage.reported")
        .collect();
    assert_eq!(reported, [entry("stage.reported", "agent")]);

    let before = d.events_of("T-0001").await.len();
    let reopen = json!({ "action": "reopen", "to": "implement" });
    assert_eq!(stage(&d, agent_token, reopen).await.0, 200);
    assert_eq!(
        stage(&d, agent_token, json!({ "action": "start" })).await.0,
        200
    );
    let after = d.events_of("T-0001").await.split_off(before);
    assert!(
        after.contains(&entry("stage.reopened", "agent")),
        "{after:?}"
    );
    assert!(
        after.iter().all(|(_, source)| source != "human"),
        "{after:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn may_not_change_the_marks_of_a_stage() {
    let rows = [
        ("skip", json!({ "stage": "review", "on": true })),
        ("ci", json!({ "stage": "implement", "on": false })),
        ("workflow", json!({ "stage": "implement", "on": false })),
    ];
    for (path, body) in rows {
        let d = task().await;
        let agent_token: &str = &d.agent_token("T-0001");
        let tasks = d.call(Method::GET, "/tasks", Some(TOKEN)).await;

        let (status, _) = put(&d, agent_token, path, body).await;

        assert_eq!(status, 403, "{path}");
        assert_eq!(
            d.call(Method::GET, "/tasks", Some(TOKEN)).await,
            tasks,
            "{path}"
        );
    }

    // The fourth row of 'may not change $url': the settings of a project.
    let d = task().await;
    let agent_token: &str = &d.agent_token("T-0001");
    let tasks = d.call(Method::GET, "/tasks", Some(TOKEN)).await;
    let settings = d
        .call(Method::GET, "/repos/default/settings", Some(TOKEN))
        .await;
    let body = json!({ "agents": { "permissionMode": "ask" } });

    let (status, _) = d
        .send(
            Method::PUT,
            "/repos/default/settings",
            Some(agent_token),
            &body,
        )
        .await;

    assert_eq!(status, 403);
    assert_eq!(d.call(Method::GET, "/tasks", Some(TOKEN)).await, tasks);
    assert_eq!(
        d.call(Method::GET, "/repos/default/settings", Some(TOKEN))
            .await,
        settings
    );
}

/// `natamux stage pass` as a command stage's script sends it, with its one-time ticket.
async fn pass_with(d: &TestDaemon, token: &str, ticket: &str) -> u16 {
    let body = json!({ "action": "pass", "stage": "implement" });
    let headers = [("x-natamux-stage-ticket", ticket)];
    let path = "/tasks/T-0001/stage";
    d.exchange_with(Method::POST, path, Some(token), &headers, Some(&body))
        .await
        .0
}

// Security T-0224 (S-01): the agent token passes a stage only with the stage's ticket, once.
#[tokio::test(flavor = "multi_thread")]
async fn passes_a_stage_only_with_its_one_time_ticket() {
    let d = running().await;
    let agent_token: &str = &d.agent_token("T-0001");
    let pass = json!({ "action": "pass", "stage": "implement" });
    assert_eq!(stage(&d, agent_token, pass).await.0, 403);
    assert_eq!(pass_with(&d, agent_token, "wrong").await, 403);
    let tickets = &d.daemon.state.stages.run.options.stage_tickets;
    let other_stage = tickets.issue("T-0001", "review");
    assert_eq!(pass_with(&d, agent_token, &other_stage).await, 403);

    let ticket = tickets.issue("T-0001", "implement");
    assert_eq!(pass_with(&d, agent_token, &ticket).await, 200);
    let events = d.events_of("T-0001").await;
    assert!(
        events.contains(&entry("stage.passed", "agent")),
        "{events:?}"
    );
    assert_eq!(pass_with(&d, agent_token, &ticket).await, 403);
}
