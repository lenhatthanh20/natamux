//! `describe('changes and fileDiff')`.

use std::fs;

use natamux_daemon::tasks::errors::TaskError;
use natamux_daemon::workspaces::changes::GIT_READ_MS;
use natamux_git::exec::spawn_count;
use natamux_proto::workspaces::changes::{ChangeStatus, FileDiff, ImageSide};
use natamux_proto::workspaces::workspace::ChangeEntry;

use super::*;

fn task_with_workspace(fx: &Fx) -> (Task, PathBuf) {
    let task = fx.with_workspace("Fix the bug");
    let workspace = PathBuf::from(task.workspace.clone().unwrap());
    (task, workspace)
}

fn paths_of(fx: &Fx, task_id: &str) -> Vec<String> {
    let changes = fx.ws.changes(task_id, false).unwrap();
    changes.into_iter().map(|entry| entry.path).collect()
}

#[test]
fn lists_the_files_that_changed_in_the_workspace() {
    let fx = Fx::new();
    let (task, workspace) = task_with_workspace(&fx);
    assert!(fx.ws.changes(&task.id, false).unwrap().is_empty());

    fs::write(workspace.join("notes.txt"), "hello\n").unwrap();
    fx.move_clock(GIT_READ_MS);

    let expected = ChangeEntry {
        path: "notes.txt".into(),
        status: ChangeStatus::Untracked,
        original_path: None,
        added: Some(1),
        removed: Some(0),
    };
    assert_eq!(fx.ws.changes(&task.id, false).unwrap(), [expected]);
}

// Node compares the promises (`toBe`); here a read that ran again would show the new file.
#[test]
fn keeps_the_list_for_a_moment_then_reads_it_again() {
    let fx = Fx::new();
    let (task, workspace) = task_with_workspace(&fx);
    let first = fx.ws.changes(&task.id, false).unwrap();
    fs::write(workspace.join("notes.txt"), "hello\n").unwrap();

    fx.move_clock(GIT_READ_MS - 1);
    assert_eq!(fx.ws.changes(&task.id, false).unwrap(), first);
    fx.move_clock(1);
    assert_eq!(paths_of(&fx, &task.id), ["notes.txt"]);
}

#[test]
fn reads_git_now_when_asked_for_a_fresh_list_and_keeps_that_one() {
    let fx = Fx::new();
    let (task, workspace) = task_with_workspace(&fx);
    fx.ws.changes(&task.id, false).unwrap();
    fs::write(workspace.join("notes.txt"), "hello\n").unwrap();

    let fresh = fx.ws.changes(&task.id, true).unwrap();
    let names: Vec<_> = fresh.iter().map(|entry| entry.path.as_str()).collect();
    assert_eq!(names, ["notes.txt"]);
    assert_eq!(fx.ws.changes(&task.id, false).unwrap(), fresh);
}

// Node: two callers get the same promise. Here two threads at the same time cost the git
// processes of one read, and get the same list.
#[test]
fn shares_one_read_between_callers_that_ask_at_the_same_time() {
    let fx = Fx::exclusive();
    let (task, workspace) = task_with_workspace(&fx);
    fs::write(workspace.join("notes.txt"), "hello\n").unwrap();
    fx.ws.touch(&task.id);
    let before = spawn_count();
    fx.ws.changes(&task.id, false).unwrap();
    let one = spawn_count() - before;
    assert!(one > 0);

    fx.ws.touch(&task.id);
    let before = spawn_count();
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let asks: Vec<_> = (0..2)
        .map(|_| {
            let (ws, id, barrier) = (Arc::clone(&fx.ws), task.id.clone(), Arc::clone(&barrier));
            std::thread::spawn(move || {
                barrier.wait();
                ws.changes(&id, false).unwrap()
            })
        })
        .collect();
    let lists: Vec<_> = asks.into_iter().map(|ask| ask.join().unwrap()).collect();

    assert_eq!(lists[0], lists[1]);
    assert_eq!(spawn_count() - before, one);
}

#[test]
fn gives_both_sides_of_a_changed_file() {
    let fx = Fx::new();
    let (task, workspace) = task_with_workspace(&fx);
    fs::write(workspace.join("notes.txt"), "hello\n").unwrap();

    let expected = FileDiff {
        path: "notes.txt".into(),
        status: ChangeStatus::Untracked,
        original: None,
        modified: Some("hello\n".into()),
        binary: false,
        too_large: false,
    };
    assert_eq!(fx.ws.file_diff(&task.id, "notes.txt").unwrap(), expected);
}

fn refuses_a_path_that_is_not_a_changed_file(path: &str) {
    let fx = Fx::new();
    let (task, _) = task_with_workspace(&fx);
    assert!(is_not_changed(&fx.ws.file_diff(&task.id, path)));
}

#[test]
fn refuses_missing_txt_which_is_not_a_changed_file() {
    refuses_a_path_that_is_not_a_changed_file("missing.txt");
}

#[test]
fn refuses_a_path_into_the_git_folder_of_the_repository_which_is_not_a_changed_file() {
    refuses_a_path_that_is_not_a_changed_file("../repo/.git/config");
}

#[test]
fn refuses_an_absolute_path_which_is_not_a_changed_file() {
    refuses_a_path_that_is_not_a_changed_file("/etc/passwd");
}

#[test]
fn refuses_a_double_dot_which_is_not_a_changed_file() {
    refuses_a_path_that_is_not_a_changed_file("..");
}

#[test]
fn refuses_a_file_that_was_changed_and_then_put_back() {
    let fx = Fx::new();
    let (task, workspace) = task_with_workspace(&fx);
    fs::write(workspace.join("notes.txt"), "hello\n").unwrap();
    assert_eq!(fx.ws.changes(&task.id, false).unwrap().len(), 1);
    fs::remove_file(workspace.join("notes.txt")).unwrap();
    // As a hook or shell of the task does after a change.
    fx.ws.touch(&task.id);

    assert!(is_not_changed(&fx.ws.file_diff(&task.id, "notes.txt")));
}

#[test]
fn reuses_the_kept_list_of_changes_for_the_diff_of_a_file() {
    let fx = Fx::exclusive();
    let (task, workspace) = task_with_workspace(&fx);
    fs::write(workspace.join("tracked.txt"), "one\n").unwrap();
    run_git(&workspace, &["add", "tracked.txt"]);
    run_git(&workspace, &["commit", "-m", "add tracked"]);
    fs::write(workspace.join("tracked.txt"), "two\n").unwrap();
    fx.ws.changes(&task.id, false).unwrap();

    let before = spawn_count();
    let diff = fx.ws.file_diff(&task.id, "tracked.txt").unwrap();
    assert_eq!(diff.modified.as_deref(), Some("two\n"));
    // `cat-file -s` and `show` only, no git status.
    assert_eq!(spawn_count() - before, 2);
}

#[test]
fn reads_the_changes_again_for_a_file_that_changed_after_the_kept_list() {
    let fx = Fx::new();
    let (task, workspace) = task_with_workspace(&fx);
    fx.ws.changes(&task.id, false).unwrap();
    fs::write(workspace.join("notes.txt"), "hello\n").unwrap();

    let diff = fx.ws.file_diff(&task.id, "notes.txt").unwrap();
    assert_eq!(diff.modified.as_deref(), Some("hello\n"));
}

#[test]
fn gives_one_side_of_a_changed_image_and_refuses_an_image_that_did_not_change() {
    let fx = Fx::new();
    let (task, workspace) = task_with_workspace(&fx);
    let png: [u8; 5] = [0x89, 0x50, 0x4e, 0x47, 0];
    fs::write(workspace.join("logo.png"), png).unwrap();

    let (kind, bytes) = fx
        .ws
        .file_image(&task.id, "logo.png", ImageSide::Modified)
        .unwrap();

    assert_eq!(kind, "image/png");
    assert_eq!(bytes, png);
    assert!(is_not_changed(&fx.ws.file_image(
        &task.id,
        "other.png",
        ImageSide::Modified
    )));
}

#[test]
fn refuses_a_task_that_has_no_workspace() {
    let fx = Fx::new();
    let task = fx.task("Fix the bug");

    assert!(is_conflict(&fx.ws.changes(&task.id, false)));
    assert!(is_conflict(&fx.ws.file_diff(&task.id, "a.txt")));
}

#[test]
fn throws_task_not_found_error_for_an_unknown_task() {
    let fx = Fx::new();
    let result = fx.ws.changes("T-0099", false);
    assert!(matches!(
        result,
        Err(WorkspaceError::Task(TaskError::NotFound(_)))
    ));
}
