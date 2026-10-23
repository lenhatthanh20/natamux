//! The task routes of a running Rust daemon: the `GET /jobs`, `tasks`, `edit a task` and
//! `templates` groups of `server.test.ts`, and the task cases of `authentication`, `terminal` and
//! `ended shells`.

use hyper::Method;
use serde_json::{Value, json};

use crate::server::{TOKEN, TestDaemon, bare, daemon};

async fn create(d: &TestDaemon, body: Value) -> (u16, Value) {
    d.send(Method::POST, "/tasks", Some(TOKEN), &body).await
}

async fn get(d: &TestDaemon, path: &str) -> (u16, Value) {
    d.call(Method::GET, path, Some(TOKEN)).await
}

fn ids(body: &Value) -> Vec<String> {
    body["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|task| task["id"].as_str().unwrap().to_string())
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn lists_the_background_jobs_that_run_now() {
    let d = bare("jobs").await;
    let jobs = std::sync::Arc::clone(&d.daemon.state.jobs);
    let running = tokio::spawn(async move {
        jobs.run(
            "push",
            "Pushing",
            Some("T-0001"),
            std::future::pending::<()>(),
        )
        .await
    });
    crate::server::eventually(|| !d.daemon.state.jobs.list().is_empty()).await;

    let (status, body) = get(&d, "/jobs").await;

    assert_eq!(status, 200);
    assert_eq!(body, json!({ "jobs": d.daemon.state.jobs.list() }));
    assert_eq!(body["jobs"][0]["label"], "Pushing");
    assert_eq!(body["jobs"][0]["taskId"], "T-0001");
    running.abort();
}

#[tokio::test(flavor = "multi_thread")]
async fn needs_the_token_for_jobs() {
    let d = bare("jobs-token").await;
    assert_eq!(d.call(Method::GET, "/jobs", None).await.0, 401);
    assert_eq!(get(&d, "/jobs").await, (200, json!({ "jobs": [] })));
}

#[tokio::test(flavor = "multi_thread")]
async fn does_not_accept_the_token_in_the_query_string_outside_events() {
    let d = bare("query-token").await;
    let path = format!("/tasks?token={TOKEN}");
    assert_eq!(d.call(Method::GET, &path, None).await.0, 401);
}

#[tokio::test(flavor = "multi_thread")]
async fn takes_the_token_in_the_query_string_on_events_only() {
    let d = bare("events-token").await;
    let mut dashboard = d.dashboard(TOKEN).await;
    assert!(dashboard.open_after(50).await);
    let url = format!("ws://{}/events?token=nope", d.daemon.address);
    assert!(
        async_tungstenite::tokio::connect_async(url.as_str())
            .await
            .is_err()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn creates_a_task_and_reads_it_back() {
    let d = bare("create").await;
    let (status, created) = create(&d, json!({ "title": "Fix the dashboard" })).await;
    assert_eq!(status, 201);
    assert_eq!(created["task"]["id"], "T-0001");
    assert_eq!(created["task"]["title"], "Fix the dashboard");

    let (status, fetched) = get(&d, "/tasks/T-0001").await;
    assert_eq!(status, 200);
    assert_eq!(fetched["task"]["id"], "T-0001");
    assert_eq!(ids(&get(&d, "/tasks").await.1), ["T-0001"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn takes_the_id_from_the_title_and_refuses_an_id_that_is_used_with_400() {
    let d = bare("title-id").await;
    let (status, created) = create(&d, json!({ "title": "Task-0005: Fix bug xxx" })).await;
    assert_eq!(status, 201);
    assert_eq!(created["task"]["id"], "TASK-0005");
    assert_eq!(created["task"]["title"], "Fix bug xxx");
    let (_, fetched) = get(&d, "/tasks/TASK-0005").await;
    assert_eq!(fetched["task"]["title"], "Fix bug xxx");

    let (status, clash) = create(&d, json!({ "title": "task-0005 Other" })).await;
    assert_eq!(status, 400);
    assert_eq!(clash["message"], "Task id TASK-0005 is already used");
    assert_eq!(ids(&get(&d, "/tasks").await.1).len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn rejects_an_invalid_task_body_with_400() {
    let d = bare("invalid").await;
    let (status, body) = create(&d, json!({ "title": "" })).await;
    assert_eq!(status, 400);
    assert_eq!(body["error"], "invalid_request");
    assert_eq!(body["issues"].as_array().unwrap().len(), 1);
    assert!(body["message"].as_str().unwrap().starts_with("title: "));
}

#[tokio::test(flavor = "multi_thread")]
async fn rejects_a_missing_body_with_400() {
    let d = bare("no-body").await;
    let (status, body) = d.call(Method::POST, "/tasks", Some(TOKEN)).await;
    assert_eq!(status, 400);
    // The whole body is wrong, so the message names no field.
    assert!(!body["message"].as_str().unwrap().starts_with(':'));
}

#[tokio::test(flavor = "multi_thread")]
async fn takes_a_task_title_of_up_to_400_characters() {
    let d = bare("long-title").await;
    assert_eq!(create(&d, json!({ "title": "a".repeat(400) })).await.0, 201);
    let (status, long) = create(&d, json!({ "title": "a".repeat(401) })).await;
    assert_eq!(status, 400);
    assert!(long["message"].as_str().unwrap().starts_with("title: "));
}

#[tokio::test(flavor = "multi_thread")]
async fn links_an_issue_by_an_http_or_https_url_only() {
    let d = bare("issue").await;
    create(&d, json!({ "title": "Linked" })).await;
    let d = &d;
    let link = |url: &'static str| async move {
        let body = json!({ "provider": "jira", "key": "ABC-1", "url": url });
        d.send(Method::PUT, "/tasks/T-0001/issue", Some(TOKEN), &body)
            .await
    };

    for url in ["vscode://file/etc/passwd", "file:///etc/passwd"] {
        let (status, refused) = link(url).await;
        assert_eq!(status, 400);
        assert_eq!(refused["error"], "invalid_request");
        assert_eq!(refused["issues"][0]["path"], json!(["url"]));
    }
    assert_eq!(
        get(d, "/tasks/T-0001").await.1["task"]["issue"],
        Value::Null
    );

    let (status, linked) = link("https://jira.example.com/browse/ABC-1").await;
    assert_eq!(status, 200);
    assert_eq!(
        linked["task"]["issue"]["url"],
        "https://jira.example.com/browse/ABC-1"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn deletes_a_task_and_the_tasks_that_pointed_at_it_stay() {
    let d = bare("delete").await;
    create(&d, json!({ "title": "Release" })).await;
    create(&d, json!({ "title": "Notes", "parentId": "T-0001" })).await;

    let (status, deleted) = d.call(Method::DELETE, "/tasks/T-0001", Some(TOKEN)).await;

    assert_eq!(status, 200);
    assert_eq!(deleted["id"], "T-0001");
    assert_eq!(deleted["tasks"][0]["id"], "T-0002");
    assert_eq!(deleted["tasks"][0]["parentId"], Value::Null);
    assert_eq!(ids(&get(&d, "/tasks").await.1), ["T-0002"]);
    assert_eq!(get(&d, "/tasks/T-0001").await.0, 404);
}

#[tokio::test(flavor = "multi_thread")]
async fn returns_404_when_deleting_an_unknown_task() {
    let d = bare("delete-unknown").await;
    let status = d.call(Method::DELETE, "/tasks/T-0099", Some(TOKEN)).await.0;
    assert_eq!(status, 404);
}

#[tokio::test(flavor = "multi_thread")]
async fn returns_404_for_an_unknown_task() {
    let d = bare("unknown").await;
    assert_eq!(get(&d, "/tasks/T-0099").await.0, 404);
}

#[tokio::test(flavor = "multi_thread")]
async fn returns_400_for_a_task_id_that_is_not_valid() {
    let d = bare("bad-id").await;
    assert_eq!(get(&d, "/tasks/nope").await.0, 400);
}

#[tokio::test(flavor = "multi_thread")]
async fn rejects_an_unknown_state_filter_with_400() {
    let d = bare("bad-state").await;
    assert_eq!(get(&d, "/tasks?state=finished").await.0, 400);
}

#[tokio::test(flavor = "multi_thread")]
async fn answers_404_for_the_terminal_routes_of_an_unknown_task() {
    let d = daemon("terminal-404", true).await;
    for (method, path) in [
        (Method::POST, "/tasks/T-0042/terminal/ticket"),
        (Method::GET, "/tasks/T-0042/terminals"),
    ] {
        let (status, body) = d.call(method.clone(), path, Some(TOKEN)).await;
        assert_eq!(
            (status, body["error"].as_str()),
            (404, Some("not_found")),
            "{method} {path}"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn answers_404_for_the_ended_shells_of_an_unknown_task() {
    let d = daemon("ended-404", true).await;
    assert_eq!(get(&d, "/tasks/T-0042/ended-shells").await.0, 404);
}

#[tokio::test(flavor = "multi_thread")]
async fn changes_the_details_and_refuses_bad_changes_and_unknown_tasks() {
    let d = bare("edit").await;
    create(&d, json!({ "title": "Fix the bug" })).await;
    let d = &d;
    let patch = |id: &'static str, body: Value| async move {
        d.send(Method::PATCH, &format!("/tasks/{id}"), Some(TOKEN), &body)
            .await
    };

    let (status, edited) = patch(
        "T-0001",
        json!({ "title": "Fix the parser", "labels": ["parser"] }),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(edited["task"]["title"], "Fix the parser");
    assert_eq!(edited["task"]["labels"], json!(["parser"]));

    assert_eq!(
        patch("T-0001", json!({ "title": "a".repeat(400) })).await.0,
        200
    );
    let (status, long) = patch("T-0001", json!({ "title": "a".repeat(401) })).await;
    assert_eq!(status, 400);
    assert!(long["message"].as_str().unwrap().starts_with("title: "));

    assert_eq!(patch("T-0001", json!({ "state": "done" })).await.0, 400);
    assert_eq!(
        patch("T-0001", json!({ "dependsOn": ["T-0001"] })).await.0,
        400
    );
    assert_eq!(patch("T-0099", json!({ "title": "x" })).await.0, 404);
}

#[tokio::test(flavor = "multi_thread")]
async fn saves_a_template_and_makes_a_task_from_it() {
    let d = bare("templates").await;
    let (status, saved) = d
        .send(
            Method::POST,
            "/templates",
            Some(TOKEN),
            &json!({ "name": "Bug fix", "title": "Fix", "description": "Steps", "labels": ["bug"] }),
        )
        .await;
    assert_eq!(status, 201);
    assert_eq!(saved["template"]["id"], "bug-fix");

    let (_, listed) = get(&d, "/templates").await;
    assert_eq!(listed["templates"][0]["id"], "bug-fix");
    assert_eq!(listed["templates"][0]["name"], "Bug fix");
    assert_eq!(listed["templates"].as_array().unwrap().len(), 1);

    let d = &d;
    let from = |id: &'static str, body: Value| async move {
        d.send(
            Method::POST,
            &format!("/templates/{id}/tasks"),
            Some(TOKEN),
            &body,
        )
        .await
    };
    let (status, created) = from("bug-fix", json!({ "title": "Fix the login" })).await;
    assert_eq!(status, 201);
    assert_eq!(created["task"]["title"], "Fix the login");
    assert_eq!(created["task"]["description"], "Steps");
    assert_eq!(created["task"]["labels"], json!(["bug"]));

    assert_eq!(from("nope", json!({ "title": "x" })).await.0, 404);
    let (status, elsewhere) = from("bug-fix", json!({ "title": "x", "repo": "nope" })).await;
    assert_eq!(status, 404);
    assert_eq!(
        elsewhere["message"],
        "Repository nope is not known. Add it first"
    );
}
