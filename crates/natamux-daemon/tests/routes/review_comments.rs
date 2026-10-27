//! The review comment routes: add, list with pins, patch, delete and send.

use std::path::PathBuf;

use hyper::Method;
use natamux_core::git::review_file::send_prompt;
use natamux_git::test_support::run_git;
use serde_json::{Value, json};

use crate::git_repo::git_daemon;
use crate::server::{AGENT_TOKEN, TOKEN, TestDaemon, eventually};
use crate::socket::Socket;

const BASE: &str = "/tasks/T-0001/review-comments";
const CODE: &str = "a\nb\nc\nd\ne\n";

// T-0001 with a real worktree that has `app.ts` committed.
async fn ready(name: &str) -> (TestDaemon, PathBuf) {
    let d = git_daemon(name).await;
    let workspace = d.task_with_workspace().await;
    d.settle();
    std::fs::write(workspace.join("app.ts"), CODE).unwrap();
    run_git(&workspace, &["add", "app.ts"]);
    run_git(&workspace, &["commit", "-m", "add app"]);
    (d, workspace)
}

fn input(side: &str, start: u32, end: u32, body: &str) -> Value {
    json!({ "path": "app.ts", "side": side, "start": start, "end": end, "body": body })
}

async fn add(d: &TestDaemon, side: &str, start: u32, end: u32) -> (u16, Value) {
    d.post(BASE, &input(side, start, end, "Rename it")).await
}

async fn patch(d: &TestDaemon, id: &str, body: &Value) -> (u16, Value) {
    let path = format!("{BASE}/{id}");
    d.send(Method::PATCH, &path, Some(TOKEN), body).await
}

async fn comments(d: &TestDaemon) -> Vec<Value> {
    let (status, list) = d.get(BASE).await;
    assert_eq!(status, 200, "{list}");
    list["comments"].as_array().unwrap().clone()
}

// A running agent in Terminal 1: its terminal stays open while the socket does.
async fn with_agent(d: &TestDaemon) -> Socket {
    let mut socket = d.open("cols=80&rows=24").await;
    socket.until(|socket| socket.replayed()).await;
    d.ptys()[0].set_program("node");
    socket
}

#[tokio::test(flavor = "multi_thread")]
async fn adds_a_draft_with_the_quote_the_daemon_read_and_tells_dashboards() {
    let (d, workspace) = ready("rc-add").await;
    std::fs::write(workspace.join("app.ts"), "a\nb\nc\nd\nNEW\n").unwrap();
    let mut dashboard = d.dashboard(TOKEN).await;
    let bus = std::sync::Arc::clone(&d.daemon.state.bus);
    eventually(|| bus.size() == 1).await;

    let (status, added) = add(&d, "new", 2, 3).await;

    assert_eq!(status, 201, "{added}");
    let comment = &added["comment"];
    assert_eq!(comment["id"], "c1");
    assert_eq!(comment["state"], "draft");
    assert_eq!(comment["quote"], "b\nc");
    assert_eq!(comment["body"], "Rename it");
    // The old side is the last commit, whatever the file is now.
    let (status, old) = add(&d, "old", 5, 5).await;
    assert_eq!(status, 201, "{old}");
    assert_eq!(old["comment"]["quote"], "e");
    let (status, whole) = add(&d, "new", 0, 0).await;
    assert_eq!(status, 201, "{whole}");
    assert_eq!(whole["comment"]["quote"], "");
    let listed = comments(&d).await;
    let ids: Vec<&str> = listed.iter().map(|c| c["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["c3", "c1", "c2"]);
    assert!(listed.iter().all(|c| c["outdated"].is_null()));
    let told = json!({ "kind": "review-comments.changed", "taskId": "T-0001" });
    dashboard
        .until(|socket| socket.controls().contains(&told))
        .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn refuses_lines_that_are_not_in_the_file_and_bad_input() {
    let (d, _) = ready("rc-bad").await;

    for (start, end) in [(3, 2), (1, 9), (0, 2), (2, 0)] {
        let (status, body) = add(&d, "new", start, end).await;
        assert_eq!(status, 400, "{start}-{end}: {body}");
    }
    let mut missing = input("new", 1, 1, "x");
    missing["path"] = json!("nothing.ts");
    assert_eq!(d.post(BASE, &missing).await.0, 400);
    let long = "x".repeat(5001);
    for body in [json!(long), json!("  "), json!(7)] {
        let mut bad = input("new", 1, 1, "x");
        bad["body"] = body;
        let (status, answer) = d.post(BASE, &bad).await;
        assert_eq!((status, &answer["error"]), (400, &json!("invalid_request")));
    }
    let mut side = input("new", 1, 1, "x");
    side["side"] = json!("both");
    assert_eq!(d.post(BASE, &side).await.0, 400);
    assert_eq!(d.get("/tasks/T-0099/review-comments").await.0, 404);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_agent_token_gets_403_on_every_route() {
    let (d, _) = ready("rc-agent").await;
    assert_eq!(add(&d, "new", 1, 1).await.0, 201);
    let body = input("new", 1, 1, "x");
    let one = format!("{BASE}/c1");
    let send = format!("{BASE}/send");

    for (method, path) in [
        (Method::GET, BASE),
        (Method::POST, BASE),
        (Method::PATCH, one.as_str()),
        (Method::DELETE, one.as_str()),
        (Method::POST, send.as_str()),
    ] {
        let (status, answer) = d.send(method, path, Some(AGENT_TOKEN), &body).await;
        assert_eq!((status, &answer["error"]), (403, &json!("forbidden")), "{path}");
    }
    assert_eq!(comments(&d).await.len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn changes_the_text_of_a_draft_and_the_state_of_a_sent_comment_only() {
    let (d, _) = ready("rc-patch").await;
    let _agent = with_agent(&d).await;
    assert_eq!(add(&d, "new", 2, 2).await.0, 201);

    let (status, edited) = patch(&d, "c1", &json!({ "body": "  Better  " })).await;
    assert_eq!((status, &edited["comment"]["body"]), (200, &json!("Better")));
    assert_eq!(patch(&d, "c1", &json!({ "state": "resolved" })).await.0, 409);
    assert_eq!(patch(&d, "c1", &json!({})).await.0, 400);
    assert_eq!(patch(&d, "c9", &json!({ "body": "x" })).await.0, 404);
    assert_eq!(d.post_empty(&format!("{BASE}/send")).await.0, 200);

    assert_eq!(patch(&d, "c1", &json!({ "body": "Late" })).await.0, 409);
    let (status, done) = patch(&d, "c1", &json!({ "state": "resolved" })).await;
    assert_eq!((status, &done["comment"]["state"]), (200, &json!("resolved")));
    assert!(done["comment"]["resolvedAt"].is_string());
    assert_eq!(patch(&d, "c1", &json!({ "state": "resolved" })).await.0, 409);
    let (status, back) = patch(&d, "c1", &json!({ "state": "sent" })).await;
    assert_eq!((status, &back["comment"]["state"]), (200, &json!("sent")));
    assert!(back["comment"]["resolvedAt"].is_null());
}

#[tokio::test(flavor = "multi_thread")]
async fn deletes_a_comment_and_never_reuses_its_id() {
    let (d, _) = ready("rc-delete").await;
    assert_eq!(add(&d, "new", 1, 1).await.0, 201);

    assert_eq!(d.delete(&format!("{BASE}/c1")).await.0, 204);
    assert_eq!(d.delete(&format!("{BASE}/c1")).await.0, 404);
    assert!(comments(&d).await.is_empty());

    let (_, next) = add(&d, "new", 1, 1).await;
    assert_eq!(next["comment"]["id"], "c2");
}

#[tokio::test(flavor = "multi_thread")]
async fn follows_the_code_when_lines_come_in_above_it_and_marks_a_rewrite_outdated() {
    let (d, workspace) = ready("rc-pin").await;
    assert_eq!(add(&d, "new", 2, 3).await.0, 201);
    let mut dashboard = d.dashboard(TOKEN).await;
    let bus = std::sync::Arc::clone(&d.daemon.state.bus);
    eventually(|| bus.size() == 1).await;

    std::fs::write(workspace.join("app.ts"), format!("{}{CODE}", "x\n".repeat(10))).unwrap();
    let moved = comments(&d).await;

    assert_eq!((&moved[0]["start"], &moved[0]["end"]), (&json!(12), &json!(13)));
    assert!(moved[0]["outdated"].is_null());
    // A pin that moved is told to dashboards, and kept in the file.
    let told = json!({ "kind": "review-comments.changed", "taskId": "T-0001" });
    dashboard
        .until(|socket| socket.controls().contains(&told))
        .await;
    std::fs::write(workspace.join("app.ts"), "a\nB\nC\nd\ne\n").unwrap();
    let rewritten = comments(&d).await;
    assert_eq!(rewritten.len(), 1);
    assert_eq!(rewritten[0]["outdated"], true);
    assert_eq!((&rewritten[0]["start"], &rewritten[0]["quote"]), (&json!(12), &json!("b\nc")));
}

#[tokio::test(flavor = "multi_thread")]
async fn refuses_a_send_with_no_draft_or_no_agent_and_keeps_the_drafts() {
    let (d, _) = ready("rc-send-409").await;
    let send = format!("{BASE}/send");
    assert_eq!(d.post_empty(&send).await.0, 409);
    assert_eq!(add(&d, "new", 1, 1).await.0, 201);

    // Only a shell runs: nothing may be pasted into it.
    let (status, answer) = d.post_empty(&send).await;

    assert_eq!(status, 409, "{answer}");
    assert_eq!(comments(&d).await[0]["state"], "draft");
}

#[tokio::test(flavor = "multi_thread")]
async fn sends_every_draft_in_one_prompt_and_records_one_event() {
    let (d, workspace) = ready("rc-send").await;
    assert_eq!(add(&d, "new", 1, 2).await.0, 201);
    assert_eq!(add(&d, "new", 4, 4).await.0, 201);
    let _agent = with_agent(&d).await;

    let (status, sent) = d.post_empty(&format!("{BASE}/send")).await;

    assert_eq!(status, 200, "{sent}");
    assert_eq!(sent["task"]["id"], "T-0001");
    let states: Vec<&Value> = sent["comments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| &c["state"])
        .collect();
    assert_eq!(states, [&json!("sent"), &json!("sent")]);
    assert!(sent["comments"][0]["sentAt"].is_string());
    let file = std::fs::read_to_string(workspace.join(".natamux-task/review-comments.md")).unwrap();
    assert!(file.contains("## New") && file.contains("### c1") && file.contains("### c2"));
    let pty = d.ptys()[0].clone();
    let prompt = format!("\x1b[200~{}\x1b[201~", send_prompt(2));
    eventually(|| pty.written() == [prompt.as_str(), "\r"]).await;
    let (_, events) = d.get("/tasks/T-0001/events").await;
    let commented: Vec<&Value> = events["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| &e["event"])
        .filter(|e| e["type"] == "review.commented")
        .collect();
    assert_eq!(commented.len(), 1);
    assert_eq!(commented[0]["payload"]["comments"], json!(["c1", "c2"]));
    assert_eq!(comments(&d).await[1]["state"], "sent");
}
