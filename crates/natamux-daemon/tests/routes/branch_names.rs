//! The `branch` field of `POST /workspaces`, `POST /tasks/:id/workspace` and
//! `POST /tasks/:id/agent`, and the project's `git.branchName`.

use natamux_git::test_support::run_git;
use serde_json::{Value, json};

use crate::git_repo::git_daemon;
use crate::server::{TestDaemon, bare};

async fn task_count(d: &TestDaemon) -> usize {
    d.get("/tasks").await.1["tasks"].as_array().unwrap().len()
}

fn assert_branch_issue(status: u16, body: &Value) {
    assert_eq!(status, 400, "{body}");
    assert_eq!(body["error"], "invalid_request", "{body}");
    assert_eq!(body["issues"][0]["path"], json!(["branch"]), "{body}");
}

#[tokio::test(flavor = "multi_thread")]
async fn refuses_a_new_workspace_with_a_branch_name_git_does_not_take_and_makes_no_task() {
    let d = git_daemon("branch-bad-new").await;

    for branch in ["bad..name", "", "-x", "a b"] {
        let body = json!({ "title": "Example", "branch": branch });
        let (status, answer) = d.post("/workspaces", &body).await;
        assert_branch_issue(status, &answer);
    }

    assert_eq!(task_count(&d).await, 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn makes_a_new_workspace_on_the_named_branch() {
    let d = git_daemon("branch-new").await;

    let body = json!({ "title": "Example", "branch": "feature-x" });
    let (status, answer) = d.post("/workspaces", &body).await;

    assert_eq!(status, 202, "{answer}");
    d.settle();
    let (_, task) = d.get("/tasks/T-0001").await;
    assert_eq!(task["task"]["branch"], "feature-x");
}

#[tokio::test(flavor = "multi_thread")]
async fn names_the_branch_of_a_task_after_the_pattern_of_the_project() {
    let d = bare("branch-pattern").await;
    let settings = json!({ "git": { "branchName": "{id}" } }).to_string();
    std::fs::write(d.repo().join(".natamux.json"), settings).unwrap();
    d.make_git_repo();
    d.make_task("ABC-12: Fix it").await;

    let (status, created) = d.post_empty("/tasks/ABC-12/workspace").await;

    assert_eq!(status, 201, "{created}");
    assert_eq!(created["task"]["branch"], "ABC-12");
}

#[tokio::test(flavor = "multi_thread")]
async fn makes_the_workspace_of_a_task_on_the_named_branch() {
    let d = git_daemon("branch-task").await;
    d.make_task("Fix the bug").await;

    let body = json!({ "branch": "feature-x" });
    let (status, created) = d.post("/tasks/T-0001/workspace", &body).await;

    assert_eq!(status, 201, "{created}");
    assert_eq!(created["task"]["branch"], "feature-x");
}

#[tokio::test(flavor = "multi_thread")]
async fn refuses_a_branch_checked_out_elsewhere_with_409_and_leaves_no_workspace() {
    let d = git_daemon("branch-checked-out").await;
    run_git(&d.repo(), &["checkout", "-b", "feature-x"]);
    d.make_task("Fix the bug").await;

    let body = json!({ "branch": "feature-x" });
    let (status, answer) = d.post("/tasks/T-0001/workspace", &body).await;

    assert_eq!(status, 409, "{answer}");
    let message = answer["message"].as_str().unwrap_or_default();
    assert!(
        message.starts_with("Branch feature-x is checked out in "),
        "{answer}"
    );
    let (_, task) = d.get("/tasks/T-0001").await;
    assert!(task["task"]["workspace"].is_null(), "{task}");
}

#[tokio::test(flavor = "multi_thread")]
async fn refuses_a_bad_branch_name_for_the_workspace_of_a_task_and_for_an_agent() {
    let d = git_daemon("branch-bad-task").await;
    d.make_task("Fix the bug").await;

    let body = json!({ "branch": "bad..name" });
    let (status, answer) = d.post("/tasks/T-0001/workspace", &body).await;
    assert_branch_issue(status, &answer);
    let body = json!({ "agent": "claude-code", "branch": "bad..name" });
    let (status, answer) = d.post("/tasks/T-0001/agent", &body).await;
    assert_branch_issue(status, &answer);

    let (_, task) = d.get("/tasks/T-0001").await;
    assert!(task["task"]["workspace"].is_null(), "{task}");
}
