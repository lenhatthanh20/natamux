//! The `graph`, `commit and push`, `rollback` and `branches` groups of `server.test.ts`.

use std::path::PathBuf;

use hyper::Method;
use natamux_git::test_support::run_git;
use serde_json::{Value, json};

use crate::git_repo::git_daemon;
use crate::server::TestDaemon;

/// T-0001 with its workspace and one new file, `fix.txt`.
async fn with_change(name: &str) -> (TestDaemon, PathBuf) {
    let d = git_daemon(name).await;
    let workspace = d.task_with_workspace().await;
    std::fs::write(workspace.join("fix.txt"), "fixed\n").unwrap();
    (d, workspace)
}

#[tokio::test(flavor = "multi_thread")]
async fn gives_the_history_of_the_task_branch() {
    let d = git_daemon("graph").await;
    d.task_with_workspace().await;

    let (status, graph) = d.get("/tasks/T-0001/graph").await;

    assert_eq!(status, 200, "{graph}");
    assert_eq!(graph["base"], "main");
    assert_eq!(graph["ahead"], 0);
    assert_eq!(graph["behind"], 0);
    assert_eq!(graph["push"], json!({ "remote": "none" }));
    assert_eq!(graph["truncated"], false);
    assert_eq!(graph["commits"].as_array().unwrap().len(), 1);
    let (_, fresh) = d.get("/tasks/T-0001/graph?fresh=1").await;
    assert_eq!(fresh["base"], "main");
    assert_eq!(fresh["ahead"], 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn answers_409_for_a_task_without_a_workspace_and_needs_the_token() {
    let d = git_daemon("graph-409").await;
    d.make_task("Fix the bug").await;

    assert_eq!(d.get("/tasks/T-0001/graph").await.0, 409);
    let (status, _) = d.call(Method::GET, "/tasks/T-0001/graph", None).await;
    assert_eq!(status, 401);
}

#[tokio::test(flavor = "multi_thread")]
async fn commits_the_changes_of_the_workspace() {
    let (d, _) = with_change("commit").await;

    let (status, done) = d
        .post(
            "/tasks/T-0001/commit",
            &json!({ "message": "  Fix the bug  " }),
        )
        .await;

    assert_eq!(status, 200, "{done}");
    let hash = done["hash"].as_str().unwrap();
    assert!(
        hash.len() == 40 && hash.chars().all(|c| c.is_ascii_hexdigit()),
        "{hash}"
    );
    assert_eq!(done["task"]["id"], "T-0001");
    let (status, _) = d
        .post("/tasks/T-0001/commit", &json!({ "message": "Again" }))
        .await;
    assert_eq!(status, 409);
}

#[tokio::test(flavor = "multi_thread")]
async fn answers_400_for_an_empty_message() {
    let (d, _) = with_change("commit-empty").await;

    let (status, _) = d
        .post("/tasks/T-0001/commit", &json!({ "message": "   " }))
        .await;

    assert_eq!(status, 400);
}

#[tokio::test(flavor = "multi_thread")]
async fn pushes_to_origin_and_answers_409_when_there_is_none() {
    let (d, _) = with_change("push").await;
    assert_eq!(d.post_empty("/tasks/T-0001/push").await.0, 409);

    d.add_origin();

    let (status, pushed) = d.post_empty("/tasks/T-0001/push").await;
    assert_eq!(status, 200, "{pushed}");
    assert_eq!(pushed["task"]["id"], "T-0001");
}

// Node fakes `runAgentOnce`. Here the daemon runs a script as its `claude`, which needs `sh`.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn drafts_a_commit_message_and_answers_409_when_nothing_changed() {
    let d = crate::fake_tools::with_fake_claude("draft", false).await;
    d.make_git_repo();
    let workspace = d.task_with_workspace().await;
    std::fs::write(workspace.join("fix.txt"), "fixed\n").unwrap();

    let (status, draft) = d.post_empty("/tasks/T-0001/commit-message").await;

    assert_eq!(status, 200, "{draft}");
    assert_eq!(
        draft,
        json!({ "message": "Add the fix", "source": "agent", "agent": "claude-code" })
    );
    std::fs::remove_file(workspace.join("fix.txt")).unwrap();
    assert_eq!(d.post_empty("/tasks/T-0001/commit-message").await.0, 409);
    let (status, unknown) = d.post_empty("/tasks/T-0099/commit-message").await;
    assert_eq!(status, 404);
    assert_eq!(unknown["error"], "not_found");
}

async fn event_count(d: &TestDaemon) -> usize {
    d.get("/tasks/T-0001/events").await.1["events"]
        .as_array()
        .unwrap()
        .len()
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn ignores_a_body_and_answers_200_with_the_simple_draft_when_the_agent_fails() {
    let d = crate::fake_tools::with_fake_claude("draft-fails", true).await;
    d.make_git_repo();
    let workspace = d.task_with_workspace().await;
    std::fs::write(workspace.join("fix.txt"), "fixed\n").unwrap();
    let before = event_count(&d).await;

    let (status, draft) = d.post("/tasks/T-0001/commit-message", &json!({})).await;

    assert_eq!(status, 200, "{draft}");
    // The failure text names the program as it was started: the path of the script.
    let reason = format!("{} exited with code 1: boom", d.fake_claude().display());
    assert_eq!(
        draft,
        json!({
            "message": "Add fix.txt", "source": "draft", "agent": "claude-code", "reason": reason,
        })
    );
    assert_eq!(event_count(&d).await, before);
}

#[tokio::test(flavor = "multi_thread")]
async fn needs_the_token() {
    let d = git_daemon("git-token").await;
    let urls = [
        "/tasks/T-0001/commit",
        "/tasks/T-0001/commit-message",
        "/tasks/T-0001/push",
        "/tasks/T-0001/pull-request",
    ];
    for url in urls {
        assert_eq!(d.call(Method::POST, url, None).await.0, 401, "{url}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn rolls_back_changed_files_and_undoes_it_once() {
    let (d, workspace) = with_change("rollback").await;

    let (status, done) = d
        .post("/tasks/T-0001/rollback", &json!({ "paths": ["fix.txt"] }))
        .await;

    assert_eq!(status, 200, "{done}");
    assert_eq!(done, json!({ "files": 1 }));
    assert!(!workspace.join("fix.txt").exists());
    let (status, undone) = d.post_empty("/tasks/T-0001/rollback/undo").await;
    assert_eq!(status, 200, "{undone}");
    assert_eq!(undone, json!({ "files": 1 }));
    let text = std::fs::read_to_string(workspace.join("fix.txt")).unwrap();
    assert_eq!(text, "fixed\n");
    assert_eq!(d.post_empty("/tasks/T-0001/rollback/undo").await.0, 409);
}

#[tokio::test(flavor = "multi_thread")]
async fn answers_400_for_a_bad_list_and_404_for_a_file_that_did_not_change() {
    let (d, _) = with_change("rollback-400").await;
    let bad: [Value; 5] = [
        json!({}),
        json!({ "paths": [] }),
        json!({ "paths": [""] }),
        json!({ "paths": ["x".repeat(1025)] }),
        json!({ "paths": vec!["fix.txt"; 5001] }),
    ];

    for (n, payload) in bad.iter().enumerate() {
        assert_eq!(
            d.post("/tasks/T-0001/rollback", payload).await.0,
            400,
            "list {n}"
        );
    }

    let body = json!({ "paths": ["fix.txt", "other.txt"] });
    let (status, missing) = d.post("/tasks/T-0001/rollback", &body).await;
    assert_eq!(status, 404, "{missing}");
    assert_eq!(missing["error"], "not_found");
}

#[tokio::test(flavor = "multi_thread")]
async fn needs_the_token_for_a_rollback() {
    let d = git_daemon("rollback-token").await;
    for url in ["/tasks/T-0001/rollback", "/tasks/T-0001/rollback/undo"] {
        assert_eq!(d.call(Method::POST, url, None).await.0, 401, "{url}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn lists_branches_and_makes_a_workspace_from_one() {
    let d = git_daemon("branches").await;

    let (_, listed) = d.get("/git/branches").await;

    assert_eq!(
        listed,
        json!({ "branches": ["main"], "current": "main", "defaultBranch": "main" })
    );
    d.make_task("Fix the bug").await;
    let (status, created) = d
        .post("/tasks/T-0001/workspace", &json!({ "base": "main" }))
        .await;
    assert_eq!(status, 201, "{created}");
    d.make_task("Another").await;
    let (status, _) = d
        .post("/tasks/T-0002/workspace", &json!({ "base": "nope" }))
        .await;
    assert_eq!(status, 409);
}

#[tokio::test(flavor = "multi_thread")]
async fn lists_the_branches_of_origin() {
    let d = git_daemon("remote-branches").await;

    let (status, none) = d.get("/git/remote-branches").await;
    assert_eq!(status, 200, "{none}");
    assert_eq!(none, json!({ "branches": [] }));

    d.add_origin();
    run_git(
        &d.repo(),
        &["push", "origin", "main", "main:refs/heads/dev"],
    );
    let (_, listed) = d.get("/git/remote-branches").await;
    assert_eq!(listed, json!({ "branches": ["origin/dev", "origin/main"] }));

    let (status, _) = d.get("/git/remote-branches?repo=..").await;
    assert_eq!(status, 400);
}
