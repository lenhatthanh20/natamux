//! `sync_branch`: the task follows the branch its agent switched the worktree to, and a remove
//! deletes that branch, never a main one.

use super::remove::path_of;
use super::*;

const SWITCHED: &str = "feature-x";
const CHANGED: &str = "workspace.branch-changed";

fn changes(fx: &Fx, task_id: &str) -> Vec<StoredEvent> {
    fx.events(task_id)
        .into_iter()
        .filter(|stored| stored.event.kind == CHANGED)
        .collect()
}

#[test]
fn records_the_branch_the_agent_switched_to_once() {
    let fx = Fx::new();
    let task = fx.with_workspace("Fix the bug");
    run_git(&path_of(&task), &["switch", "-c", SWITCHED]);

    let synced = fx.ws.sync_branch(&task.id).unwrap();
    let again = fx.ws.sync_branch(&task.id).unwrap();

    assert_eq!(synced.branch.as_deref(), Some(SWITCHED));
    assert_eq!(again.branch.as_deref(), Some(SWITCHED));
    let changes = changes(&fx, &task.id);
    assert_eq!(changes.len(), 1);
    let event = &changes[0].event;
    assert_eq!(event.source, "system");
    assert_eq!(event.payload["from"], BRANCH);
    assert_eq!(event.payload["to"], SWITCHED);
}

#[test]
fn keeps_the_branch_while_head_is_detached() {
    let fx = Fx::new();
    let task = fx.with_workspace("Fix the bug");
    run_git(&path_of(&task), &["switch", "--detach"]);

    let synced = fx.ws.sync_branch(&task.id).unwrap();

    assert_eq!(synced.branch.as_deref(), Some(BRANCH));
    assert!(changes(&fx, &task.id).is_empty());
}

#[test]
fn never_takes_the_default_branch() {
    let fx = Fx::new();
    let task = fx.with_workspace("Fix the bug");
    let worktree = path_of(&task);
    run_git(&worktree, &["switch", "--ignore-other-worktrees", "main"]);

    let synced = fx.ws.sync_branch(&task.id).unwrap();

    assert_eq!(synced.branch.as_deref(), Some(BRANCH));
    assert!(changes(&fx, &task.id).is_empty());
}

#[test]
fn never_takes_the_branch_of_the_main_checkout() {
    let fx = Fx::new();
    let task = fx.with_workspace("Fix the bug");
    run_git(&fx.repo, &["switch", "-c", "side"]);
    run_git(
        &path_of(&task),
        &["switch", "--ignore-other-worktrees", "side"],
    );

    let synced = fx.ws.sync_branch(&task.id).unwrap();

    assert_eq!(synced.branch.as_deref(), Some(BRANCH));
    assert!(changes(&fx, &task.id).is_empty());
}

#[test]
fn keeps_the_branch_of_a_task_whose_folder_is_gone() {
    let fx = Fx::new();
    let task = fx.with_workspace("Fix the bug");
    let worktree = path_of(&task);
    run_git(&worktree, &["switch", "-c", SWITCHED]);
    delete_folder(&fx, &worktree, false);

    let synced = fx.ws.sync_branch(&task.id).unwrap();

    assert_eq!(synced.branch.as_deref(), Some(BRANCH));
    assert!(changes(&fx, &task.id).is_empty());
}

#[test]
fn leaves_a_task_without_a_workspace_alone() {
    let fx = Fx::new();
    let task = fx.task("Fix the bug");

    let synced = fx.ws.sync_branch(&task.id).unwrap();

    assert_eq!(synced.branch, None);
    assert!(changes(&fx, &task.id).is_empty());
}

#[test]
fn remove_deletes_the_branch_the_agent_switched_to_and_keeps_the_old_one() {
    let fx = Fx::new();
    let task = fx.with_workspace("Fix the bug");
    run_git(&path_of(&task), &["switch", "-c", SWITCHED]);

    let result = fx.ws.remove(&task.id, true).unwrap();

    assert!(result.branch_deleted);
    assert_eq!(result.task.branch, None);
    assert!(!fx.branch_exists(SWITCHED));
    assert!(fx.branch_exists(BRANCH));
    assert_eq!(changes(&fx, &task.id).len(), 1);
}

#[test]
fn remove_for_delete_deletes_the_branch_the_agent_switched_to() {
    let fx = Fx::new();
    let task = fx.with_workspace("Fix the bug");
    let worktree = path_of(&task);
    run_git(&worktree, &["switch", "-c", SWITCHED]);
    run_git(&worktree, &["commit", "--allow-empty", "-m", "work"]);

    let synced = fx.ws.sync_branch(&task.id).unwrap();
    let result = fx.ws.remove_for_delete(&task.id, false).unwrap();
    fx.ws.delete_branches_of(&synced).unwrap();

    assert!(result.branch_deleted);
    assert!(!fx.branch_exists(SWITCHED));
    assert!(fx.branch_exists(BRANCH));
}

/// A task whose old record holds `main` while the main checkout is on another branch, so git
/// itself would let `main` be deleted.
fn task_on_main(fx: &Fx) -> Task {
    let task = fx.with_workspace("Fix the bug");
    run_git(&fx.repo, &["switch", "-c", "side"]);
    run_git(&path_of(&task), &["switch", "--detach"]);
    fx.tasks
        .record_branch(&task.id, Some(BRANCH.to_string()), "main".to_string())
        .unwrap()
}

#[test]
fn remove_never_deletes_the_default_branch_from_an_old_record() {
    let fx = Fx::new();
    let task = task_on_main(&fx);

    let result = fx.ws.remove_for_delete(&task.id, false).unwrap();

    assert!(!result.branch_deleted);
    assert!(fx.branch_exists("main"));
}

#[test]
fn delete_branches_of_never_deletes_the_default_branch() {
    let fx = Fx::new();
    let task = task_on_main(&fx);
    let removed = fx.ws.remove(&task.id, false).unwrap().task;
    assert_eq!(removed.branch.as_deref(), Some("main"));

    fx.ws.delete_branches_of(&removed).unwrap();

    assert!(fx.branch_exists("main"));
}
