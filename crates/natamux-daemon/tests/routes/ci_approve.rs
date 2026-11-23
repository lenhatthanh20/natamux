//! `POST /tasks/:id/stages/:stage/ci-approve` (F4): only a person approves a CI push, and only
//! the exact tree that waits for approval.

use hyper::Method;
use serde_json::json;

use crate::server::{TOKEN, TestDaemon, bare};

const APPROVE: &str = "/tasks/T-0001/stages/implement/ci-approve";

/// A task whose CI push of Implement waits for a person, as `ciPush: approved-snapshot` leaves it.
async fn waiting() -> TestDaemon {
    let d = bare("ci-approve").await;
    d.set_simple_pipeline();
    d.make_task("Example").await;
    let needed = json!({
        "stage": "implement", "mode": "approved-snapshot", "remote": "",
        "branch": "natamux-ci/t-0001", "commit": "c1", "tree": "t1", "files": ["a.rs"],
        "passes": [],
    });
    d.daemon
        .state
        .tasks
        .record_event("T-0001", "stage.ci-approval-needed", needed, "system")
        .unwrap();
    d
}

async fn kinds(d: &TestDaemon) -> Vec<String> {
    let events = d.events_of("T-0001").await;
    events.into_iter().map(|(kind, _)| kind).collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn refuses_the_agent_token_and_starts_no_round() {
    let d = waiting().await;
    let agent = d.agent_token("T-0001");
    let body = json!({ "tree": "t1" });

    let (status, _) = d.send(Method::POST, APPROVE, Some(&agent), &body).await;

    assert_eq!(status, 403);
    let kinds = kinds(&d).await;
    assert!(
        !kinds.contains(&"stage.ci-approved".to_string()),
        "{kinds:?}"
    );
    assert!(!kinds.contains(&"stage.ci-started".to_string()));
}

#[tokio::test(flavor = "multi_thread")]
async fn answers_409_for_another_tree_and_starts_the_round_for_the_shown_one() {
    let d = waiting().await;

    let wrong = json!({ "tree": "t2" });
    let (status, body) = d.send(Method::POST, APPROVE, Some(TOKEN), &wrong).await;
    assert_eq!(status, 409, "{body}");
    assert!(!kinds(&d).await.contains(&"stage.ci-approved".to_string()));

    let right = json!({ "tree": "t1" });
    let (status, body) = d.send(Method::POST, APPROVE, Some(TOKEN), &right).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["task"]["id"], "T-0001");
    let kinds = kinds(&d).await;
    let approved = kinds.iter().position(|kind| kind == "stage.ci-approved");
    let started = kinds.iter().position(|kind| kind == "stage.ci-started");
    assert!(approved.is_some() && approved < started, "{kinds:?}");

    // The approval is spent: a second one finds nothing to approve.
    let (again, _) = d.send(Method::POST, APPROVE, Some(TOKEN), &right).await;
    assert_eq!(again, 409);
}

#[tokio::test(flavor = "multi_thread")]
async fn answers_409_when_nothing_waits_for_approval() {
    let d = bare("ci-approve-none").await;
    d.set_simple_pipeline();
    d.make_task("Example").await;
    let body = json!({ "tree": "t1" });
    let (status, _) = d.send(Method::POST, APPROVE, Some(TOKEN), &body).await;
    assert_eq!(status, 409);
}
