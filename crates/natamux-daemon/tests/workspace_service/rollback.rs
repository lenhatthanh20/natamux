//! `describe('rollback and undoRollback')`.

use std::fs;

use natamux_proto::workspaces::changes::ChangeStatus;

use super::*;

fn task_with_changes(fx: &Fx) -> (Task, PathBuf) {
    let task = fx.with_workspace("Fix the bug");
    let workspace = PathBuf::from(task.workspace.clone().unwrap());
    fs::write(workspace.join("notes.txt"), "hello\n").unwrap();
    fs::write(workspace.join("keep.txt"), "keep\n").unwrap();
    (task, workspace)
}

fn paths(fx: &Fx, task_id: &str) -> Vec<String> {
    let changes = fx.ws.changes(task_id, false).unwrap();
    changes.into_iter().map(|entry| entry.path).collect()
}

// Node spies on `touch`; here the effect is checked: a rollback marks the workspace, so
// dashboards hear `workspace.dirty` once the timer runs.
#[test]
fn rolls_back_the_named_files_then_undoes_it_once() {
    let fx = Fx::new();
    let (task, workspace) = task_with_changes(&fx);
    // Kept from before the rollback, which must not show up afterwards.
    assert_eq!(paths(&fx, &task.id), ["keep.txt", "notes.txt"]);
    fx.flush_dirty();
    let before = fx.dirty_count(&task.id);

    let named = strings(&["notes.txt", "notes.txt"]);
    assert_eq!(fx.ws.rollback(&task.id, &named).unwrap(), 1);
    assert!(!workspace.join("notes.txt").exists());
    assert_eq!(paths(&fx, &task.id), ["keep.txt"]);
    fx.flush_dirty();
    assert_eq!(fx.dirty_count(&task.id), before + 1);

    assert_eq!(fx.ws.undo_rollback(&task.id).unwrap(), 1);
    assert_eq!(
        fs::read_to_string(workspace.join("notes.txt")).unwrap(),
        "hello\n"
    );
    assert_eq!(paths(&fx, &task.id), ["keep.txt", "notes.txt"]);
    assert_eq!(
        error_text(fx.ws.undo_rollback(&task.id)),
        "Nothing to undo."
    );
}

#[test]
fn forgets_the_rollback_after_a_commit_so_undo_cannot_replace_committed_files() {
    let fx = Fx::new();
    let (task, _) = task_with_changes(&fx);
    fx.ws.rollback(&task.id, &strings(&["notes.txt"])).unwrap();
    fx.ws.commit(&task.id, "Keep the rest").unwrap();

    assert_eq!(
        error_text(fx.ws.undo_rollback(&task.id)),
        "Nothing to undo."
    );
}

#[test]
fn refuses_a_workspace_without_a_commit_and_changes_nothing() {
    let fx = Fx::new();
    let (task, workspace) = task_with_changes(&fx);
    run_git(&workspace, &["checkout", "-q", "--orphan", "empty"]);

    let refusal = fx.ws.rollback(&task.id, &strings(&["notes.txt"]));
    assert!(is_conflict(&refusal));
    assert_eq!(error_text(refusal), "There is no commit to roll back to.");
    assert!(workspace.join("notes.txt").exists());
}

#[test]
fn refuses_a_file_that_is_not_changed_and_changes_nothing() {
    let fx = Fx::new();
    let (task, workspace) = task_with_changes(&fx);

    let result = fx
        .ws
        .rollback(&task.id, &strings(&["notes.txt", "other.txt"]));
    assert!(is_not_changed(&result));
    assert!(workspace.join("notes.txt").exists());
    assert!(is_conflict(&fx.ws.undo_rollback(&task.id)));
}

#[test]
fn refuses_a_file_with_a_conflict_and_changes_nothing() {
    let fx = Fx::new();
    let (task, workspace) = task_with_changes(&fx);
    run_git(&workspace, &["checkout", "-q", "-b", "side"]);
    fs::write(workspace.join("both.txt"), "side\n").unwrap();
    run_git(&workspace, &["add", "both.txt"]);
    run_git(&workspace, &["commit", "-q", "-m", "side"]);
    run_git(&workspace, &["checkout", "-q", "-"]);
    fs::write(workspace.join("both.txt"), "task\n").unwrap();
    run_git(&workspace, &["add", "both.txt"]);
    run_git(&workspace, &["commit", "-q", "-m", "task"]);
    assert!(!git_ok(&workspace, &["merge", "-q", "side"]));

    let changes = fx.ws.changes(&task.id, true).unwrap();
    let both = changes
        .iter()
        .find(|entry| entry.path == "both.txt")
        .unwrap();
    assert_eq!(both.status, ChangeStatus::Conflict);
    let result = fx
        .ws
        .rollback(&task.id, &strings(&["both.txt", "notes.txt"]));
    assert!(error_text(result).contains("both.txt has a conflict"));
    assert!(workspace.join("notes.txt").exists());
}

// The commit is held while it runs: the listener of its `jobs.changed` blocks (see `Gate`).
#[test]
fn refuses_while_another_git_action_of_the_task_runs() {
    let fx = Fx::new();
    let (task, _) = task_with_changes(&fx);
    let (jobs, _sent, gate) = Gate::jobs(&fx);
    let ws = fx.build(|options| options.jobs = jobs);
    let (runner, id) = (Arc::clone(&ws), task.id.clone());
    let commit = std::thread::spawn(move || runner.commit(&id, "Fix the bug"));
    gate.wait_started();

    let result = ws.rollback(&task.id, &strings(&["notes.txt"]));
    assert!(is_conflict(&result));

    gate.release();
    commit.join().unwrap().unwrap();
}

#[test]
fn refuses_a_task_that_has_no_workspace() {
    let fx = Fx::new();
    let task = fx.task("Fix the bug");

    assert!(is_conflict(&fx.ws.rollback(&task.id, &strings(&["a.txt"]))));
    assert!(is_conflict(&fx.ws.undo_rollback(&task.id)));
}
