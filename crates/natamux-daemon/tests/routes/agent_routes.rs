//! Security T-0224 (S-01, S-05, S-21): the agent token of a task reaches its own task, reads of
//! its direct subtasks and a few global reads; everything else answers 403. The `natamux`
//! commands an agent runs keep working (the others are in `stages.rs` and `task_events.rs`).

use std::time::Duration;

use hyper::Method;
use serde_json::json;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::time::timeout;

use crate::server::{TOKEN, TestDaemon, bare, eventually};

/// T-0001 (A), T-0002 (B, another task) and T-0003 (C, a subtask of A).
async fn three_tasks(name: &str) -> TestDaemon {
    let d = bare(name).await;
    d.make_task("A").await;
    d.make_task("B").await;
    let body = json!({ "title": "C", "parentId": "T-0001" });
    let (status, made) = d.send(Method::POST, "/tasks", Some(TOKEN), &body).await;
    assert_eq!(status, 201, "{made}");
    d
}

/// Every route of a task, as `METHOD /rest` after `/tasks/<id>`.
const TASK_ROUTES: &str = "\
GET
GET /terminal
GET /ended-shells
GET /terminals
GET /events
GET /attachments
GET /attachments/a.png
GET /review-comments
GET /files
GET /files/spec.md
GET /changes
GET /changes/file
GET /changes/image
GET /tree
GET /file
GET /graph
POST /terminal/ticket
POST /review-comments/send
POST /review-comments
POST /events
POST /stage
POST /agent
POST /agent/message
POST /agent/reopen
POST /workspace
POST /commit
POST /rollback
POST /rollback/undo
POST /fan-out
POST /commit-message
POST /push
POST /pull-request
POST /pull-request/refresh
POST /issue
PUT /issue
PUT /workflow
PUT /ci
PUT /skip
PUT /files/spec.md
PUT /file
DELETE
DELETE /terminals/2
DELETE /review-comments/c1
DELETE /workspace
PATCH
PATCH /review-comments/c1";

fn task_routes(id: &str) -> Vec<(Method, String)> {
    TASK_ROUTES
        .lines()
        .map(|line| {
            let (method, end) = line.split_once(' ').unwrap_or((line, ""));
            let method = Method::from_bytes(method.as_bytes()).unwrap();
            (method, format!("/tasks/{id}{end}"))
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn every_route_of_another_task_and_every_global_write_answers_403() {
    let d = three_tasks("agent-table").await;
    let a = d.agent_token("T-0001");
    let mut refused = task_routes("T-0002");
    for (method, path) in [
        (Method::POST, "/templates"),
        (Method::POST, "/templates/x/tasks"),
        (Method::POST, "/workspaces"),
        (Method::POST, "/repos"),
        (Method::DELETE, "/repos/default"),
        (Method::PUT, "/repos/default/settings"),
        (Method::GET, "/repos/default/trust"),
        (Method::POST, "/repos/default/trust"),
        (Method::GET, "/github/issues"),
        (Method::POST, "/github/issues/import"),
        (Method::POST, "/tasks/T-0003/stage"),
        (Method::PATCH, "/tasks/T-0003"),
        (Method::DELETE, "/tasks/T-0001"),
        (Method::DELETE, "/tasks/T-0001/workspace"),
    ] {
        refused.push((method, path.to_string()));
    }
    for (method, path) in refused {
        let (status, body) = d.send(method.clone(), &path, Some(&a), &json!({})).await;
        assert_eq!(status, 403, "{method} {path}: {body}");
    }
    let (_, tasks) = d.call(Method::GET, "/tasks", Some(TOKEN)).await;
    assert_eq!(tasks["tasks"].as_array().unwrap().len(), 3);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_allowed_routes_answer_as_for_the_user() {
    let d = three_tasks("agent-allowed").await;
    let a = d.agent_token("T-0001");
    for path in [
        "/tasks",
        "/tasks?state=draft",
        "/jobs",
        "/templates",
        "/activity",
        "/usage",
        "/limits",
        "/workspaces/preparing",
        "/repos",
        "/repos/default/settings",
        "/repos/default/skills",
        "/git/branches",
        "/git/remote-branches",
        "/events",
        "/tasks/T-0001",
        "/tasks/T-0001/events",
        "/tasks/T-0001/terminals",
        "/tasks/T-0001/files",
        "/tasks/T-0003",
        "/tasks/T-0003/events",
    ] {
        let user = d.call(Method::GET, path, Some(TOKEN)).await;
        let agent = d.call(Method::GET, path, Some(&a)).await;
        assert_ne!(agent.0, 403, "{path}");
        assert_eq!(agent.0, user.0, "{path}");
    }
    // `natamux statusline`: Claude Code's limits from its status line.
    let (limits, _) = d
        .send(Method::POST, "/limits/claude-code", Some(&a), &json!({}))
        .await;
    assert_eq!(limits, 200);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_token_of_a_deleted_task_and_a_made_up_one_answer_401() {
    let d = three_tasks("agent-deleted").await;
    let b = d.agent_token("T-0002");
    let (deleted, _) = d.call(Method::DELETE, "/tasks/T-0002", Some(TOKEN)).await;
    assert_eq!(deleted, 200);
    assert_eq!(d.call(Method::GET, "/tasks", Some(&b)).await.0, 401);
    let forged = format!("T-0001.{}", b.split_once('.').unwrap().1);
    assert_eq!(d.call(Method::GET, "/tasks", Some(&forged)).await.0, 401);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_terminal_ticket_only_for_its_own_task() {
    let d = three_tasks("agent-ticket").await;
    d.give_workspace("T-0001");
    d.give_workspace("T-0002");
    let a = d.agent_token("T-0001");
    let (other, _) = d
        .call(Method::POST, "/tasks/T-0002/terminal/ticket", Some(&a))
        .await;
    assert_eq!(other, 403);
    let (own, body) = d
        .call(Method::POST, "/tasks/T-0001/terminal/ticket", Some(&a))
        .await;
    assert_eq!(own, 200, "{body}");
    assert!(body["ticket"].is_string());
}

// `natamux task create --parent <own>`: a subtask of its own task only.
#[tokio::test(flavor = "multi_thread")]
async fn makes_a_subtask_of_its_own_task_only() {
    let d = three_tasks("agent-create").await;
    let a = d.agent_token("T-0001");
    let body = json!({ "title": "D", "parentId": "T-0001" });
    let (made, task) = d.send(Method::POST, "/tasks", Some(&a), &body).await;
    assert_eq!(made, 201, "{task}");
    assert_eq!(task["task"]["parentId"], "T-0001");
    for refused in [
        json!({ "title": "E", "parentId": "T-0002" }),
        json!({ "title": "E" }),
        json!({ "title": "E", "parentId": null }),
    ] {
        let (status, _) = d.send(Method::POST, "/tasks", Some(&a), &refused).await;
        assert_eq!(status, 403, "{refused}");
    }
}

fn hook(cwd: &str) -> serde_json::Value {
    json!({ "hook_event_name": "UserPromptSubmit", "cwd": cwd })
}

async fn state_of(d: &TestDaemon, id: &str) -> String {
    let (_, body) = d
        .call(Method::GET, &format!("/tasks/{id}"), Some(TOKEN))
        .await;
    body["task"]["state"].as_str().unwrap().to_string()
}

// A hook with A's token that names B, in the header and by its folder, moves only A.
#[tokio::test(flavor = "multi_thread")]
async fn a_hook_with_the_agent_token_changes_only_its_own_task() {
    let d = three_tasks("agent-hook").await;
    let b_folder = d.give_workspace("T-0002");
    let cwd = b_folder.to_string_lossy().into_owned();
    let a = d.agent_token("T-0001");
    let header = [("x-natamux-task", "T-0002")];

    let (status, _) = d
        .hook("claude-code", "", &header, Some(&a), &hook(&cwd))
        .await;

    assert_eq!(status, 200);
    assert_eq!(state_of(&d, "T-0001").await, "running");
    assert_eq!(state_of(&d, "T-0002").await, "draft");
    // The daemon token still finds the task by the header.
    d.hook("claude-code", "", &header, Some(TOKEN), &hook(&cwd))
        .await;
    assert_eq!(state_of(&d, "T-0002").await, "running");
}

/// A `POST /tasks` whose body never comes: it holds a big-body place until it is dropped.
async fn stalled(d: &TestDaemon) -> TcpStream {
    let mut stream = TcpStream::connect(d.daemon.address).await.unwrap();
    let head = format!(
        "POST /tasks HTTP/1.1\r\nhost: localhost\r\nauthorization: Bearer {TOKEN}\r\n\
         content-type: application/json\r\ncontent-length: 1000\r\n\r\n{{"
    );
    stream.write_all(head.as_bytes()).await.unwrap();
    stream
}

// S-21: at most two big bodies are read at once; a third waits for a place.
#[tokio::test(flavor = "multi_thread")]
async fn a_third_big_body_waits_until_one_of_two_ends() {
    let d = bare("agent-big-bodies").await;
    let first = stalled(&d).await;
    let _second = stalled(&d).await;
    let places = std::sync::Arc::clone(&d.daemon.state.big_bodies);
    eventually(|| places.available_permits() == 0).await;

    let body = json!({ "title": "Third" });
    let mut third = std::pin::pin!(d.send(Method::POST, "/tasks", Some(TOKEN), &body));
    let waited = timeout(Duration::from_millis(300), &mut third).await;
    assert!(waited.is_err(), "the third did not wait");

    drop(first);
    let (status, made) = timeout(Duration::from_secs(5), third).await.unwrap();
    assert_eq!(status, 201, "{made}");
}
