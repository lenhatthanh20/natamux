//! The `pull request status` and `GitHub issues` groups of `server.test.ts`. Node hands the daemon
//! a `gh` function; the Rust daemon runs `gh` from the PATH, so these tests use the fake of
//! `fake_tools.rs` (Unix only).

use serde_json::json;

use crate::server::bare;

#[tokio::test(flavor = "multi_thread")]
async fn asks_github_now_for_a_task_with_a_pull_request() {
    let d = bare("pr-status").await;
    d.make_task("Fix the bug").await;

    let (status, body) = d.post_empty("/tasks/T-0001/pull-request/refresh").await;

    // The task has no workspace and no pull request: there is nothing to ask.
    assert_eq!(status, 200, "{body}");
    assert!(body["task"]["prStatus"].is_null());
}

// What Node's stub answers for its watcher: the pull request is merged.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn reads_the_status_of_a_linked_pull_request_through_gh() {
    let d = bare("pr-status-gh").await;
    d.fake_gh();
    d.gh_answer(
        "pr view",
        r#"{"state":"MERGED","isDraft":false,"statusCheckRollup":[]}"#,
    );
    d.make_task("Fix the bug").await;
    let url = "https://github.com/o/r/pull/5";
    let tasks = d.daemon.state.tasks.clone();
    tasks
        .record_git("T-0001", "pr.opened", json!({ "url": url }))
        .unwrap();

    let (status, body) = d.post_empty("/tasks/T-0001/pull-request/refresh").await;

    assert_eq!(status, 200, "{body}");
    assert_eq!(body["task"]["prStatus"]["state"], "merged");
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn lists_open_issues_and_imports_them_as_tasks() {
    let d = bare("issues").await;
    d.fake_gh();
    let url = "https://github.com/o/r/issues/7";
    let issues = json!([{ "number": 7, "title": "From GitHub", "body": "", "url": url }]);
    d.gh_answer("issue list", &issues.to_string());

    let (status, list) = d.get("/github/issues").await;

    assert_eq!(status, 200, "{list}");
    let first = &list["issues"][0];
    assert_eq!(first["number"], 7);
    assert_eq!(first["title"], "From GitHub");
    assert!(first["taskId"].is_null());
    let (status, imported) = d
        .post("/github/issues/import", &json!({ "numbers": [7] }))
        .await;
    assert_eq!(status, 200, "{imported}");
    assert_eq!(imported["tasks"][0]["id"], "T-0001");
    assert_eq!(imported["tasks"][0]["title"], "From GitHub");
}

#[tokio::test(flavor = "multi_thread")]
async fn answers_400_without_numbers() {
    let d = bare("issues-400").await;

    let (status, _) = d
        .post("/github/issues/import", &json!({ "numbers": [] }))
        .await;

    assert_eq!(status, 400);
}

// Node (`workspace-publish.ts`): the body of the pull request opens the task's issue first
// (`createIssue: on-pr`); when `gh issue create` fails, the request fails and no pull request is
// opened. A regression test: the Rust daemon once fell back to a body without the issue.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn opens_no_pull_request_when_its_issue_cannot_be_opened() {
    let d = crate::git_repo::git_daemon("pr-issue-fails").await;
    let tracker = r#"{"tracker":{"type":"github","createIssue":"on-pr","linkInPr":true}}"#;
    std::fs::write(d.repo().join(".natamux.json"), tracker).unwrap();
    d.task_with_workspace().await;
    d.add_origin();
    // No answer for `issue create`: the fake fails as a gh that cannot reach GitHub.
    d.fake_gh();

    let (status, body) = d.post_empty("/tasks/T-0001/pull-request").await;

    assert_eq!(status, 409, "{body}");
    let message = body["message"].as_str().unwrap_or_default();
    assert!(message.starts_with("GitHub CLI failed: "), "{body}");
    let calls = d.gh_calls();
    assert!(
        calls.iter().any(|call| call.starts_with("issue create")),
        "{calls:?}"
    );
    assert!(
        !calls.iter().any(|call| call.starts_with("pr create")),
        "{calls:?}"
    );
    let (_, task) = d.get("/tasks/T-0001").await;
    assert!(task["task"]["pr"].is_null(), "{task}");
}
