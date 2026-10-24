//! `describe('remove')`, part 3: a task without a workspace, a path that is not a worktree, and
//! worktrees with changes.

use std::fs;

use super::remove::path_of;
use super::*;

#[test]
fn refuses_a_task_that_has_no_workspace() {
    let fx = Fx::new();
    let task = fx.task("Fix the bug");

    assert!(is_conflict(&fx.ws.remove(&task.id, true)));
}

// Each one puts something at the path that is not a worktree of the repository.
fn refuses_a_path_that_is_not_a_worktree(make: impl Fn(&Fx, &Path)) {
    let fx = Fx::new();
    let task = fx.task("Fix the bug");
    let created = fx.ws.create(&task.id, None).unwrap();
    let path = path_of(&created);
    delete_folder(&fx, &path, true);
    make(&fx, &path);
    let before = contents_of(&path);

    let error = fx.ws.remove(&task.id, true).unwrap_err();

    assert!(matches!(error, WorkspaceError::Conflict(_)));
    assert!(error.to_string().contains(path_text(&path)));
    assert!(error.to_string().contains("not a git worktree"));
    assert!(fx.closed_ids().is_empty());
    assert_eq!(contents_of(&path), before);
    assert_eq!(fx.workspace_of(&task.id), created.workspace);
    assert!(
        !fx.event_kinds(&task.id)
            .contains(&"workspace.removed".to_string())
    );
    assert!(fx.branch_exists(BRANCH));
}

#[test]
fn refuses_a_path_that_is_a_folder_with_a_file_instead_of_a_worktree_and_leaves_it_alone() {
    refuses_a_path_that_is_not_a_worktree(|_, path| {
        fs::create_dir(path).unwrap();
        fs::write(path.join("notes.txt"), "my own files").unwrap();
    });
}

#[test]
fn refuses_a_path_that_is_an_empty_folder_instead_of_a_worktree_and_leaves_it_alone() {
    refuses_a_path_that_is_not_a_worktree(|_, path| fs::create_dir(path).unwrap());
}

#[test]
fn refuses_a_path_that_is_a_worktree_of_another_repository_instead_of_a_worktree_and_leaves_it_alone()
 {
    refuses_a_path_that_is_not_a_worktree(|fx, path| {
        let other = fx.root.path().join("other");
        run_git(fx.root.path(), &["init", "-b", "main", path_text(&other)]);
        run_git(&other, &["commit", "--allow-empty", "-m", "other"]);
        run_git(&other, &["worktree", "add", path_text(path)]);
    });
}

#[test]
fn refuses_a_path_that_is_a_git_repository_of_its_own_instead_of_a_worktree_and_leaves_it_alone() {
    refuses_a_path_that_is_not_a_worktree(|fx, path| {
        run_git(fx.root.path(), &["init", path_text(path)]);
    });
}

#[test]
fn refuses_a_path_that_is_a_file_instead_of_a_worktree_and_leaves_it_alone() {
    refuses_a_path_that_is_not_a_worktree(|_, path| fs::write(path, "not a folder").unwrap());
}

// Commits a file in the worktree, so that a change to it is a change to a tracked file.
fn commit_tracked_file(path: &Path) {
    fs::write(path.join("tracked.txt"), "first").unwrap();
    run_git(path, &["add", "tracked.txt"]);
    run_git(path, &["commit", "-m", "tracked"]);
}

fn edit_and_add_file(path: &Path) {
    fs::write(path.join("tracked.txt"), "edited").unwrap();
    fs::write(path.join("new.txt"), "new").unwrap();
}

fn stage_file(path: &Path) {
    fs::write(path.join("staged.txt"), "staged").unwrap();
    run_git(path, &["add", "staged.txt"]);
}

fn refuses_a_worktree_with_changes(delete_branch: bool, change: fn(&Path)) {
    let fx = Fx::new();
    let task = fx.task("Fix the bug");
    let path = path_of(&fx.ws.create(&task.id, None).unwrap());
    commit_tracked_file(&path);
    change(&path);
    let status = run_git(&path, &["status", "--porcelain"]);

    assert!(is_conflict(&fx.ws.remove(&task.id, delete_branch)));

    assert_eq!(run_git(&path, &["status", "--porcelain"]), status);
    assert!(fx.closed_ids().is_empty());
    let now = fx.tasks.get_task(&task.id).unwrap();
    assert_eq!(now.workspace, Some(path_text(&path).to_string()));
    assert_eq!(now.branch.as_deref(), Some(BRANCH));
    assert!(
        !fx.event_kinds(&task.id)
            .contains(&"workspace.removed".to_string())
    );
    assert!(fx.branch_exists(BRANCH));
}

#[test]
fn refuses_a_worktree_with_a_tracked_edit_and_a_new_file_delete_branch_false_and_leaves_it_as_it_was()
 {
    refuses_a_worktree_with_changes(false, edit_and_add_file);
}

#[test]
fn refuses_a_worktree_with_a_tracked_edit_and_a_new_file_delete_branch_true_and_leaves_it_as_it_was()
 {
    refuses_a_worktree_with_changes(true, edit_and_add_file);
}

#[test]
fn refuses_a_worktree_with_a_staged_change_only_delete_branch_false_and_leaves_it_as_it_was() {
    refuses_a_worktree_with_changes(false, stage_file);
}
