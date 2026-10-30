//! `describe('remove')`, part 2: the `stage files` group, and the workspaces whose folder was
//! deleted outside Natamux (`deleteFolder`, in its `pruned` and `still listed by git` forms).

use std::fs;

use natamux_git::worktrees::{WorktreeState, worktree_state};
use natamux_stages::files::exclude_stage_dir;
use natamux_store::task_files::{STAGE_DIR, archive_dir};

use super::*;

fn path_of(task: &Task) -> PathBuf {
    PathBuf::from(task.workspace.clone().unwrap())
}

fn not_removed(fx: &Fx, task_id: &str) -> bool {
    !fx.event_kinds(task_id)
        .contains(&"workspace.removed".to_string())
}

fn archived(fx: &Fx, task_id: &str) -> String {
    fs::read_to_string(archive_dir(&fx.data, task_id).join("spec.md")).unwrap()
}

// A task with a workspace, an archived `spec.md` (v1), and a `spec.md` in the stage folder.
fn with_spec(fx: &Fx, service: &WorkspaceService, text: &str) -> (Task, PathBuf) {
    let task = fx.task("Fix the bug");
    let workspace = path_of(&service.create(&task.id, None, None).unwrap());
    exclude_stage_dir(&workspace).unwrap();
    fs::create_dir_all(archive_dir(&fx.data, &task.id)).unwrap();
    fs::write(archive_dir(&fx.data, &task.id).join("spec.md"), "spec v1\n").unwrap();
    fs::create_dir_all(workspace.join(STAGE_DIR)).unwrap();
    fs::write(workspace.join(STAGE_DIR).join("spec.md"), text).unwrap();
    (task, workspace)
}

#[test]
fn stage_files_archives_the_stage_files_that_no_stage_archived_yet() {
    let fx = Fx::new();
    let (task, _) = with_spec(&fx, &fx.ws, "spec v2\n");

    fx.ws.remove(&task.id, false).unwrap();

    assert_eq!(archived(&fx, &task.id), "spec v2\n");
}

#[test]
fn stage_files_archives_a_stage_file_the_agent_wrote_while_its_shells_ended() {
    let fx = Fx::new();
    let target: Arc<Mutex<PathBuf>> = Arc::default();
    let at = Arc::clone(&target);
    let service = fx.build(|options| {
        options.close_terminals = Arc::new(move |_| {
            let workspace = at.lock().unwrap().clone();
            fs::write(workspace.join(STAGE_DIR).join("spec.md"), "spec v3\n").unwrap();
        });
    });
    let (task, workspace) = with_spec(&fx, &service, "spec v2\n");
    *target.lock().unwrap() = workspace;

    service.remove(&task.id, false).unwrap();

    assert_eq!(archived(&fx, &task.id), "spec v3\n");
}

#[test]
fn stage_files_leaves_the_archive_alone_when_the_removal_is_refused() {
    let fx = Fx::new();
    let (task, workspace) = with_spec(&fx, &fx.ws, "spec v2\n");
    fs::write(workspace.join("notes.txt"), "work in progress").unwrap();

    assert!(is_conflict(&fx.ws.remove(&task.id, false)));

    assert_eq!(archived(&fx, &task.id), "spec v1\n");
}

#[test]
fn stage_files_keeps_the_worktree_and_stops_the_agent_when_the_archive_cannot_be_written() {
    let fx = Fx::new();
    let (task, workspace) = with_spec(&fx, &fx.ws, "spec v2\n");
    fx.start_agent(&task.id);
    // A file where the archive folder should be makes the copy fail.
    fs::remove_dir_all(archive_dir(&fx.data, &task.id)).unwrap();
    fs::write(archive_dir(&fx.data, &task.id), "not a folder").unwrap();

    assert!(fx.ws.remove(&task.id, false).is_err());

    let kept = fs::read_to_string(workspace.join(STAGE_DIR).join("spec.md")).unwrap();
    assert_eq!(kept, "spec v2\n");
    let now = fx.tasks.get_task(&task.id).unwrap();
    assert_eq!(now.state, TaskState::Ready);
    assert_eq!(now.workspace, Some(path_text(&workspace).to_string()));
}

#[test]
fn clears_a_workspace_whose_folder_was_deleted_and_pruned() {
    let fx = Fx::new();
    let task = fx.task("Fix the bug");
    let created = fx.ws.create(&task.id, None, None).unwrap();
    delete_folder(&fx, &path_of(&created), true);

    let result = fx.ws.remove(&task.id, false).unwrap();

    assert!(!result.branch_deleted);
    assert_eq!(result.task.workspace, None);
    assert_eq!(result.task.branch.as_deref(), Some(BRANCH));
    assert!(fx.branch_exists(BRANCH));
    assert_eq!(
        fx.event_kinds(&task.id).last().unwrap(),
        "workspace.removed"
    );
}

#[test]
fn clears_a_deleted_workspace_when_its_parent_folder_is_gone_too() {
    let fx = Fx::new();
    let task = fx.task("Fix the bug");
    fx.ws.create(&task.id, None, None).unwrap();
    fs::remove_dir_all(&fx.worktrees).unwrap();
    run_git(&fx.repo, &["worktree", "prune"]);

    let result = fx.ws.remove(&task.id, false).unwrap();

    assert_eq!(result.task.workspace, None);
    assert_eq!(
        fx.event_kinds(&task.id).last().unwrap(),
        "workspace.removed"
    );
}

#[test]
fn refuses_a_second_remove_after_a_deleted_folder_was_cleared() {
    let fx = Fx::new();
    let task = fx.task("Fix the bug");
    let created = fx.ws.create(&task.id, None, None).unwrap();
    delete_folder(&fx, &path_of(&created), true);
    fx.ws.remove(&task.id, false).unwrap();

    assert!(is_conflict(&fx.ws.remove(&task.id, false)));
    let removed = fx
        .event_kinds(&task.id)
        .iter()
        .filter(|k| *k == "workspace.removed")
        .count();
    assert_eq!(removed, 1);
}

fn clears_a_deleted_folder_whose_branch_was_deleted_by_hand_too(delete_branch: bool) {
    let fx = Fx::new();
    let task = fx.task("Fix the bug");
    let created = fx.ws.create(&task.id, None, None).unwrap();
    delete_folder(&fx, &path_of(&created), true);
    run_git(&fx.repo, &["branch", "-D", BRANCH]);

    let result = fx.ws.remove(&task.id, delete_branch).unwrap();

    assert!(!result.branch_deleted);
    assert_eq!(result.task.workspace, None);
    assert_eq!(
        fx.event_kinds(&task.id).last().unwrap(),
        "workspace.removed"
    );
}

#[test]
fn clears_a_deleted_folder_whose_branch_was_deleted_by_hand_too_delete_branch_false() {
    clears_a_deleted_folder_whose_branch_was_deleted_by_hand_too(false);
}

#[test]
fn clears_a_deleted_folder_whose_branch_was_deleted_by_hand_too_delete_branch_true() {
    clears_a_deleted_folder_whose_branch_was_deleted_by_hand_too(true);
}

#[test]
fn clears_the_git_entry_of_a_workspace_whose_folder_was_deleted() {
    let fx = Fx::new();
    let task = fx.task("Fix the bug");
    let path = path_of(&fx.ws.create(&task.id, None, None).unwrap());
    delete_folder(&fx, &path, false);
    assert_eq!(
        worktree_state(&fx.repo, &path).unwrap(),
        WorktreeState::Stale
    );

    let result = fx.ws.remove(&task.id, false).unwrap();

    assert_eq!(result.task.workspace, None);
    assert_eq!(result.task.branch.as_deref(), Some(BRANCH));
    assert_eq!(
        worktree_state(&fx.repo, &path).unwrap(),
        WorktreeState::Missing
    );
    assert_eq!(
        fx.event_kinds(&task.id).last().unwrap(),
        "workspace.removed"
    );
}

#[test]
fn clears_the_git_entry_of_a_deleted_folder_whose_path_has_a_space() {
    let fx = Fx::new();
    let trees = fx.root.path().join("Application Support").join("worktrees");
    let service = fx.build(|options| options.worktrees_dir = trees);
    let task = fx.task("Fix the bug");
    let path = path_of(&service.create(&task.id, None, None).unwrap());
    delete_folder(&fx, &path, false);
    assert_eq!(
        worktree_state(&fx.repo, &path).unwrap(),
        WorktreeState::Stale
    );

    let result = service.remove(&task.id, false).unwrap();

    assert_eq!(result.task.workspace, None);
    assert_eq!(
        worktree_state(&fx.repo, &path).unwrap(),
        WorktreeState::Missing
    );
}

fn leaves_the_worktree_of_another_task_alone_after_a_deleted_folder(pruned: bool) {
    let fx = Fx::new();
    let task = fx.task("Fix the bug");
    let other = fx.task("Other work");
    let created = fx.ws.create(&task.id, None, None).unwrap();
    let other_path = path_of(&fx.ws.create(&other.id, None, None).unwrap());
    fs::write(other_path.join("notes.txt"), "work in progress").unwrap();
    delete_folder(&fx, &path_of(&created), pruned);

    fx.ws.remove(&task.id, false).unwrap();

    assert_eq!(
        worktree_state(&fx.repo, &other_path).unwrap(),
        WorktreeState::Present
    );
    assert!(other_path.join("notes.txt").exists());
    assert_eq!(
        fx.workspace_of(&other.id),
        Some(path_text(&other_path).to_string())
    );
}

#[test]
fn leaves_the_worktree_of_another_task_alone_after_a_deleted_folder_pruned() {
    leaves_the_worktree_of_another_task_alone_after_a_deleted_folder(true);
}

#[test]
fn leaves_the_worktree_of_another_task_alone_after_a_deleted_folder_still_listed_by_git() {
    leaves_the_worktree_of_another_task_alone_after_a_deleted_folder(false);
}

#[test]
fn refuses_a_locked_entry_of_a_deleted_folder_and_leaves_the_task_as_it_was() {
    let fx = Fx::new();
    let task = fx.task("Fix the bug");
    let path = path_of(&fx.ws.create(&task.id, None, None).unwrap());
    run_git(&fx.repo, &["worktree", "lock", path_text(&path)]);
    delete_folder(&fx, &path, false);

    assert!(error_text(fx.ws.remove(&task.id, true)).contains("locked"));

    assert_eq!(
        fx.workspace_of(&task.id),
        Some(path_text(&path).to_string())
    );
    assert!(not_removed(&fx, &task.id));
    assert_eq!(
        worktree_state(&fx.repo, &path).unwrap(),
        WorktreeState::Stale
    );
    assert!(fx.branch_exists(BRANCH));
}

fn deletes_a_merged_branch_of_a_deleted_folder(pruned: bool) {
    let fx = Fx::new();
    let task = fx.task("Fix the bug");
    let created = fx.ws.create(&task.id, None, None).unwrap();
    delete_folder(&fx, &path_of(&created), pruned);

    let result = fx.ws.remove(&task.id, true).unwrap();

    assert!(result.branch_deleted);
    assert_eq!(result.task.workspace, None);
    assert_eq!(result.task.branch, None);
    assert!(!fx.branch_exists(BRANCH));
    assert_eq!(
        fx.event_kinds(&task.id).last().unwrap(),
        "workspace.removed"
    );
}

#[test]
fn deletes_a_merged_branch_of_a_deleted_folder_pruned() {
    deletes_a_merged_branch_of_a_deleted_folder(true);
}

#[test]
fn deletes_a_merged_branch_of_a_deleted_folder_still_listed_by_git() {
    deletes_a_merged_branch_of_a_deleted_folder(false);
}

fn keeps_the_branch_of_a_deleted_folder_that_has_commits_not_merged(pruned: bool) {
    let fx = Fx::new();
    let task = fx.task("Fix the bug");
    let created = fx.ws.create(&task.id, None, None).unwrap();
    run_git(
        &path_of(&created),
        &["commit", "--allow-empty", "-m", "work"],
    );
    delete_folder(&fx, &path_of(&created), pruned);

    let result = fx.ws.remove(&task.id, true).unwrap();

    assert!(!result.branch_deleted);
    assert_eq!(result.task.workspace, None);
    assert_eq!(result.task.branch.as_deref(), Some(BRANCH));
    assert!(fx.branch_exists(BRANCH));
    let subject = run_git(&fx.repo, &["log", "-1", "--format=%s", BRANCH]);
    assert_eq!(subject.trim(), "work");
    assert_eq!(
        fx.event_kinds(&task.id).last().unwrap(),
        "workspace.removed"
    );
}

#[test]
fn keeps_the_branch_of_a_deleted_folder_that_has_commits_not_merged_pruned() {
    keeps_the_branch_of_a_deleted_folder_that_has_commits_not_merged(true);
}

#[test]
fn keeps_the_branch_of_a_deleted_folder_that_has_commits_not_merged_still_listed_by_git() {
    keeps_the_branch_of_a_deleted_folder_that_has_commits_not_merged(false);
}

fn ends_the_shells_of_a_deleted_folder_once_before_the_task_is_cleared(pruned: bool) {
    let fx = Fx::new();
    let task = fx.task("Fix the bug");
    let other = fx.task("Other work");
    let created = fx.ws.create(&task.id, None, None).unwrap();
    fx.ws.create(&other.id, None, None).unwrap();
    delete_folder(&fx, &path_of(&created), pruned);

    fx.ws.remove(&task.id, false).unwrap();

    let closed = fx.closed.lock().unwrap().clone();
    assert_eq!(closed, [(task.id.clone(), created.workspace.clone())]);
}

#[test]
fn ends_the_shells_of_a_deleted_folder_once_before_the_task_is_cleared_pruned() {
    ends_the_shells_of_a_deleted_folder_once_before_the_task_is_cleared(true);
}

#[test]
fn ends_the_shells_of_a_deleted_folder_once_before_the_task_is_cleared_still_listed_by_git() {
    ends_the_shells_of_a_deleted_folder_once_before_the_task_is_cleared(false);
}

fn makes_a_new_workspace_at_the_same_path_after_a_deleted_folder(pruned: bool) {
    let fx = Fx::new();
    let task = fx.task("Fix the bug");
    let created = fx.ws.create(&task.id, None, None).unwrap();
    run_git(
        &path_of(&created),
        &["commit", "--allow-empty", "-m", "work"],
    );
    delete_folder(&fx, &path_of(&created), pruned);
    fx.ws.remove(&task.id, true).unwrap();

    let again = fx.ws.create(&task.id, None, None).unwrap();

    assert_eq!(again.workspace, created.workspace);
    assert_eq!(again.branch.as_deref(), Some(BRANCH));
    assert_eq!(
        run_git(&path_of(&again), &["log", "-1", "--format=%s"]).trim(),
        "work"
    );
    let state = worktree_state(&fx.repo, &path_of(&again)).unwrap();
    assert_eq!(state, WorktreeState::Present);
}

#[test]
fn makes_a_new_workspace_at_the_same_path_after_a_deleted_folder_pruned() {
    makes_a_new_workspace_at_the_same_path_after_a_deleted_folder(true);
}

#[test]
fn makes_a_new_workspace_at_the_same_path_after_a_deleted_folder_still_listed_by_git() {
    makes_a_new_workspace_at_the_same_path_after_a_deleted_folder(false);
}

fn goes_on_with_the_kept_branch_when_a_deleted_folder_is_cleared_without_its_branch(pruned: bool) {
    let fx = Fx::new();
    let task = fx.task("Fix the bug");
    let created = fx.ws.create(&task.id, None, None).unwrap();
    delete_folder(&fx, &path_of(&created), pruned);
    fx.ws.remove(&task.id, false).unwrap();

    let again = fx.ws.create(&task.id, None, None).unwrap();

    assert_eq!(again.workspace, created.workspace);
    assert_eq!(again.branch.as_deref(), Some(BRANCH));
    assert!(path_of(&again).exists());
    let state = worktree_state(&fx.repo, &path_of(&again)).unwrap();
    assert_eq!(state, WorktreeState::Present);
}

#[test]
fn goes_on_with_the_kept_branch_when_a_deleted_folder_is_cleared_without_its_branch_pruned() {
    goes_on_with_the_kept_branch_when_a_deleted_folder_is_cleared_without_its_branch(true);
}

#[test]
fn goes_on_with_the_kept_branch_when_a_deleted_folder_is_cleared_without_its_branch_still_listed_by_git()
 {
    goes_on_with_the_kept_branch_when_a_deleted_folder_is_cleared_without_its_branch(false);
}

fn makes_a_new_branch_after_a_deleted_folder_whose_merged_branch_was_deleted(pruned: bool) {
    let fx = Fx::new();
    let task = fx.task("Fix the bug");
    let created = fx.ws.create(&task.id, None, None).unwrap();
    delete_folder(&fx, &path_of(&created), pruned);
    fx.ws.remove(&task.id, true).unwrap();

    let again = fx.ws.create(&task.id, None, None).unwrap();

    assert_eq!(again.workspace, created.workspace);
    assert_eq!(again.branch.as_deref(), Some(BRANCH));
    assert_eq!(
        run_git(&path_of(&again), &["log", "-1", "--format=%s"]).trim(),
        "init"
    );
    let state = worktree_state(&fx.repo, &path_of(&again)).unwrap();
    assert_eq!(state, WorktreeState::Present);
}

#[test]
fn makes_a_new_branch_after_a_deleted_folder_whose_merged_branch_was_deleted_pruned() {
    makes_a_new_branch_after_a_deleted_folder_whose_merged_branch_was_deleted(true);
}

#[test]
fn makes_a_new_branch_after_a_deleted_folder_whose_merged_branch_was_deleted_still_listed_by_git() {
    makes_a_new_branch_after_a_deleted_folder_whose_merged_branch_was_deleted(false);
}
