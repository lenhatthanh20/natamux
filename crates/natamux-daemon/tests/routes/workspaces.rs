//! The `workspaces` group of `server.test.ts`, and the new-workspace cases of `task events`. A
//! real git repository is the default project, as `createTestRepo` is in Node.

use std::path::{Path, PathBuf};

use hyper::Method;
use natamux_git::test_support::run_git;
use natamux_git::worktrees::{WorktreeState, worktree_state};
use natamux_store::task_files::archive_dir;
use serde_json::{Value, json};

use crate::git_repo::git_daemon;
use crate::server::{TOKEN, TestDaemon, bare};

const BRANCH: &str = "natamux/t-0001-fix-the-bug";

async fn with_workspace(name: &str) -> (TestDaemon, PathBuf) {
    let d = git_daemon(name).await;
    let workspace = d.task_with_workspace().await;
    (d, workspace)
}

/// The folder is gone, and git has not heard of it yet.
fn delete_folder(workspace: &Path) {
    std::fs::remove_dir_all(workspace).unwrap();
}

fn prune(d: &TestDaemon) {
    run_git(&d.repo(), &["worktree", "prune"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn creates_a_workspace_and_removes_it_again() {
    let d = git_daemon("ws-create").await;
    d.make_task("Fix the bug").await;

    let (status, created) = d.post_empty("/tasks/T-0001/workspace").await;

    assert_eq!(status, 201, "{created}");
    assert_eq!(created["task"]["branch"], BRANCH);
    let workspace = PathBuf::from(created["task"]["workspace"].as_str().unwrap());
    assert!(workspace.exists());
    let (status, removed) = d.delete("/tasks/T-0001/workspace?deleteBranch=true").await;
    assert_eq!(status, 200, "{removed}");
    assert_eq!(removed["branchDeleted"], true);
    assert!(removed["task"]["workspace"].is_null());
    assert!(removed["task"]["branch"].is_null());
    assert!(!workspace.exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn names_the_branch_and_folder_of_a_task_with_a_chosen_id_after_that_id() {
    let d = git_daemon("ws-chosen-id").await;
    d.make_task("Task-0005: Fix bug xxx").await;

    let (status, created) = d.post_empty("/tasks/TASK-0005/workspace").await;

    assert_eq!(status, 201, "{created}");
    assert_eq!(created["task"]["branch"], "natamux/task-0005-fix-bug-xxx");
    let workspace = PathBuf::from(created["task"]["workspace"].as_str().unwrap());
    assert_eq!(workspace.file_name().unwrap(), "TASK-0005");
    assert!(workspace.exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn refuses_a_new_workspace_whose_id_is_used_and_makes_no_task_or_branch() {
    let d = git_daemon("ws-id-used").await;
    d.make_task("TASK-0005: First").await;

    let (status, body) = d
        .post("/workspaces", &json!({ "title": "Task-0005: Other" }))
        .await;

    assert_eq!(status, 400, "{body}");
    assert_eq!(body["message"], "Task id TASK-0005 is already used");
    let (_, tasks) = d.get("/tasks").await;
    assert_eq!(tasks["tasks"].as_array().unwrap().len(), 1);
    assert_eq!(run_git(&d.repo(), &["branch", "--list", "natamux/*"]), "");
}

#[tokio::test(flavor = "multi_thread")]
async fn removes_a_workspace_whose_folder_was_already_deleted() {
    let (d, workspace) = with_workspace("ws-gone").await;
    delete_folder(&workspace);
    prune(&d);

    let (status, removed) = d.delete("/tasks/T-0001/workspace").await;

    assert_eq!(status, 200, "{removed}");
    assert_eq!(removed["branchDeleted"], false);
    assert!(removed["task"]["workspace"].is_null());
    assert_eq!(removed["task"]["branch"], BRANCH);
}

#[tokio::test(flavor = "multi_thread")]
async fn removes_a_workspace_whose_folder_was_deleted_with_its_merged_branch() {
    let (d, workspace) = with_workspace("ws-gone-merged").await;
    delete_folder(&workspace);
    prune(&d);

    let (status, removed) = d.delete("/tasks/T-0001/workspace?deleteBranch=true").await;

    assert_eq!(status, 200, "{removed}");
    assert_eq!(removed["branchDeleted"], true);
    assert!(removed["task"]["workspace"].is_null());
    assert!(removed["task"]["branch"].is_null());
}

#[tokio::test(flavor = "multi_thread")]
async fn removes_a_workspace_whose_folder_was_deleted_while_git_still_lists_it() {
    let (d, workspace) = with_workspace("ws-gone-listed").await;
    delete_folder(&workspace);

    let (status, removed) = d.delete("/tasks/T-0001/workspace").await;

    assert_eq!(status, 200, "{removed}");
    assert!(removed["task"]["workspace"].is_null());
    assert_eq!(removed["task"]["branch"], BRANCH);
    let state = worktree_state(&d.repo(), &workspace).unwrap();
    assert_eq!(state, WorktreeState::Missing);
}

#[tokio::test(flavor = "multi_thread")]
async fn answers_409_with_the_path_for_a_folder_that_is_not_a_git_worktree() {
    let (d, workspace) = with_workspace("ws-not-worktree").await;
    delete_folder(&workspace);
    prune(&d);
    std::fs::create_dir(&workspace).unwrap();

    let (status, removed) = d.delete("/tasks/T-0001/workspace").await;

    assert_eq!(status, 409, "{removed}");
    let message = removed["message"].as_str().unwrap();
    assert!(message.contains(workspace.to_str().unwrap()), "{message}");
    assert!(workspace.exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn answers_409_for_a_worktree_with_changes_that_are_not_committed() {
    let (d, workspace) = with_workspace("ws-dirty").await;
    std::fs::write(workspace.join("notes.txt"), "work in progress").unwrap();

    let (status, _) = d.delete("/tasks/T-0001/workspace").await;

    assert_eq!(status, 409);
    assert!(workspace.join("notes.txt").exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn removes_a_worktree_with_changes_that_are_not_committed_when_forced() {
    let (d, workspace) = with_workspace("ws-dirty-force").await;
    std::fs::write(workspace.join("notes.txt"), "work in progress").unwrap();
    std::fs::write(workspace.join("README.md"), "edited").unwrap();

    let (status, removed) = d
        .delete("/tasks/T-0001/workspace?deleteBranch=true&force=true")
        .await;

    assert_eq!(status, 200, "{removed}");
    assert!(removed["task"]["workspace"].is_null());
    assert!(!workspace.exists());
    let state = worktree_state(&d.repo(), &workspace).unwrap();
    assert_eq!(state, WorktreeState::Missing);
}

#[tokio::test(flavor = "multi_thread")]
async fn answers_409_with_the_path_for_a_folder_that_is_not_a_git_worktree_also_when_forced() {
    let (d, workspace) = with_workspace("ws-not-worktree-force").await;
    delete_folder(&workspace);
    prune(&d);
    std::fs::create_dir(&workspace).unwrap();
    std::fs::write(workspace.join("mine.txt"), "my own file").unwrap();

    let (status, removed) = d.delete("/tasks/T-0001/workspace?force=true").await;

    assert_eq!(status, 409, "{removed}");
    assert!(workspace.join("mine.txt").exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn deletes_a_task_whose_worktree_has_changes_when_forced() {
    let (d, workspace) = with_workspace("ws-delete-task-force").await;
    run_git(&workspace, &["commit", "--allow-empty", "-m", "work"]);
    std::fs::write(workspace.join("notes.txt"), "work in progress").unwrap();

    let (status, body) = d.delete("/tasks/T-0001?force=true").await;

    assert_eq!(status, 200, "{body}");
    assert!(!workspace.exists());
    assert_eq!(run_git(&d.repo(), &["branch", "--list", BRANCH]), "");
    assert_eq!(d.get("/tasks/T-0001").await.0, 404);
}

#[tokio::test(flavor = "multi_thread")]
async fn answers_400_for_a_force_value_that_is_not_true_or_false() {
    let (d, workspace) = with_workspace("ws-bad-force").await;

    assert_eq!(d.delete("/tasks/T-0001/workspace?force=yes").await.0, 400);
    assert_eq!(d.delete("/tasks/T-0001?force=yes").await.0, 400);
    assert!(workspace.exists());
    assert_eq!(d.get("/tasks/T-0001").await.0, 200);
}

#[tokio::test(flavor = "multi_thread")]
async fn deletes_a_task_and_its_worktree_and_keeps_both_when_the_worktree_has_changes() {
    let (d, workspace) = with_workspace("ws-delete-task").await;
    run_git(&workspace, &["commit", "--allow-empty", "-m", "work"]);
    std::fs::write(workspace.join("notes.txt"), "work in progress").unwrap();

    let (status, _) = d.delete("/tasks/T-0001").await;
    assert_eq!(status, 409);
    assert!(workspace.join("notes.txt").exists());
    assert_ne!(run_git(&d.repo(), &["branch", "--list", BRANCH]), "");
    assert_eq!(d.get("/tasks/T-0001").await.0, 200);

    std::fs::remove_file(workspace.join("notes.txt")).unwrap();
    let (status, body) = d.delete("/tasks/T-0001").await;
    assert_eq!(status, 200, "{body}");
    assert!(!workspace.exists());
    assert_eq!(d.get("/tasks/T-0001").await.0, 404);
}

#[tokio::test(flavor = "multi_thread")]
async fn keeps_the_earlier_stage_files_when_the_folder_was_already_deleted() {
    let (d, workspace) = with_workspace("ws-keep-files").await;
    let archive = archive_dir(&d.root.join("data"), "T-0001");
    std::fs::create_dir_all(&archive).unwrap();
    std::fs::write(archive.join("spec.md"), "# Goal\nKeep me.\n").unwrap();
    delete_folder(&workspace);

    let (status, _) = d.delete("/tasks/T-0001/workspace").await;

    assert_eq!(status, 200);
    let kept = std::fs::read_to_string(archive.join("spec.md")).unwrap();
    assert_eq!(kept, "# Goal\nKeep me.\n");
}

#[tokio::test(flavor = "multi_thread")]
async fn archives_nothing_from_a_folder_that_was_already_deleted() {
    let (d, workspace) = with_workspace("ws-archive-none").await;
    let archive = archive_dir(&d.root.join("data"), "T-0001");
    let archived = || -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(&archive) else {
            return Vec::new();
        };
        let mut names: Vec<String> = entries
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    };
    let before = archived();
    delete_folder(&workspace);

    let (status, _) = d.delete("/tasks/T-0001/workspace").await;

    assert_eq!(status, 200);
    assert_eq!(archived(), before);
}

#[tokio::test(flavor = "multi_thread")]
async fn answers_409_for_a_second_workspace() {
    let (d, _) = with_workspace("ws-second").await;

    let (status, body) = d.post_empty("/tasks/T-0001/workspace").await;

    assert_eq!(status, 409);
    assert_eq!(body["error"], "conflict");
}

#[tokio::test(flavor = "multi_thread")]
async fn answers_409_when_there_is_no_workspace_to_remove() {
    let d = git_daemon("ws-none").await;
    d.make_task("Fix the bug").await;

    assert_eq!(d.delete("/tasks/T-0001/workspace").await.0, 409);
}

#[tokio::test(flavor = "multi_thread")]
async fn answers_404_for_an_unknown_task() {
    let d = git_daemon("ws-unknown").await;

    assert_eq!(d.post_empty("/tasks/T-0099/workspace").await.0, 404);
}

#[tokio::test(flavor = "multi_thread")]
async fn answers_400_for_a_delete_branch_value_that_is_not_true_or_false() {
    let d = git_daemon("ws-bad-query").await;
    d.make_task("Fix the bug").await;

    let (status, _) = d.delete("/tasks/T-0001/workspace?deleteBranch=maybe").await;

    assert_eq!(status, 400);
}

#[tokio::test(flavor = "multi_thread")]
async fn needs_the_token() {
    let d = bare("ws-token").await;

    let (status, _) = d.call(Method::POST, "/tasks/T-0001/workspace", None).await;

    assert_eq!(status, 401);
}

async fn new_workspace(d: &TestDaemon, body: &Value) -> (u16, Value) {
    d.send(Method::POST, "/workspaces", Some(TOKEN), body).await
}

async fn task_count(d: &TestDaemon) -> usize {
    d.get("/tasks").await.1["tasks"].as_array().unwrap().len()
}

#[tokio::test(flavor = "multi_thread")]
async fn refuses_a_new_workspace_for_an_agent_the_daemon_cannot_start() {
    let d = git_daemon("new-ws-agent").await;

    let (status, _) = new_workspace(&d, &json!({ "title": "Example", "agent": "foo" })).await;

    assert_eq!(status, 400);
    assert_eq!(task_count(&d).await, 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn takes_a_workspace_title_of_up_to_400_characters_and_names_the_field() {
    let d = git_daemon("new-ws-title").await;

    let (status, long) = new_workspace(&d, &json!({ "title": "a".repeat(401) })).await;

    assert_eq!(status, 400, "{long}");
    assert_eq!(long["error"], "invalid_request");
    assert_eq!(long["issues"][0]["path"], json!(["title"]));
    assert!(long["message"].as_str().unwrap().starts_with("title: "));
    let (status, _) = new_workspace(&d, &json!({ "title": "a".repeat(400) })).await;
    assert_eq!(status, 202);
    d.settle();
}

#[tokio::test(flavor = "multi_thread")]
async fn answers_a_new_workspace_before_its_worktree_is_made_then_lists_none_preparing() {
    let d = git_daemon("new-ws-answer").await;

    let (status, body) = new_workspace(&d, &json!({ "title": "Example" })).await;

    assert_eq!(status, 202, "{body}");
    assert_eq!(body["task"]["id"], "T-0001");
    assert!(body["task"]["workspace"].is_null());
    d.settle();
    let (_, preparing) = d.get("/workspaces/preparing").await;
    assert_eq!(preparing, json!({ "preparing": {} }));
    let (_, task) = d.get("/tasks/T-0001").await;
    assert!(!task["task"]["workspace"].is_null());
}

#[tokio::test(flavor = "multi_thread")]
async fn keeps_the_skill_picked_as_the_context_of_a_new_workspace() {
    let d = git_daemon("new-ws-skill").await;
    let context = json!({ "source": "skill", "skill": "jira-ticket", "ref": "ABC-1" });

    let body = json!({ "title": "Example", "context": context });

    let (status, answer) = new_workspace(&d, &body).await;

    assert_eq!(status, 202, "{answer}");
    let (_, task) = d.get("/tasks/T-0001").await;
    assert_eq!(task["task"]["context"], context);
    d.settle();
}

#[tokio::test(flavor = "multi_thread")]
async fn refuses_a_picked_skill_that_is_empty_or_too_long_and_stores_nothing() {
    let d = git_daemon("new-ws-bad-skill").await;
    for skill in [String::new(), "   ".to_string(), "a".repeat(201)] {
        let context = json!({ "source": "skill", "skill": skill, "ref": "ABC-1" });

        let body = json!({ "title": "Example", "context": context });

        let (status, answer) = new_workspace(&d, &body).await;

        assert_eq!(status, 400, "{answer}");
    }
    assert_eq!(task_count(&d).await, 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn deleting_a_task_deletes_its_branch_with_commits_that_are_not_merged() {
    let (d, workspace) = with_workspace("ws-delete-unmerged").await;
    std::fs::write(workspace.join("work.txt"), "work").unwrap();
    run_git(&workspace, &["add", "work.txt"]);
    run_git(&workspace, &["commit", "-m", "work"]);

    let (status, body) = d.delete("/tasks/T-0001").await;

    assert_eq!(status, 200, "{body}");
    assert!(!workspace.exists());
    assert_eq!(run_git(&d.repo(), &["branch", "--list", "natamux/*"]), "");
}

#[tokio::test(flavor = "multi_thread")]
async fn deleting_a_task_without_a_workspace_deletes_its_branch() {
    let (d, _) = with_workspace("ws-delete-branch-only").await;
    let (status, _) = d.delete("/tasks/T-0001/workspace?deleteBranch=false").await;
    assert_eq!(status, 200);
    assert_ne!(run_git(&d.repo(), &["branch", "--list", "natamux/*"]), "");

    let (status, body) = d.delete("/tasks/T-0001").await;

    assert_eq!(status, 200, "{body}");
    assert_eq!(run_git(&d.repo(), &["branch", "--list", "natamux/*"]), "");
}

#[tokio::test(flavor = "multi_thread")]
async fn deleting_a_task_whose_branch_was_deleted_by_hand_works() {
    let (d, workspace) = with_workspace("ws-delete-branch-gone").await;
    delete_folder(&workspace);
    prune(&d);
    run_git(&d.repo(), &["branch", "-D", BRANCH]);

    let (status, body) = d.delete("/tasks/T-0001").await;

    assert_eq!(status, 200, "{body}");
}
