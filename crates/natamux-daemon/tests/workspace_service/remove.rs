//! `describe('remove')`, part 1: branches, the state of the task, and worktrees that are clean or
//! have ignored files. The refusals (a path that is not a worktree, worktrees with changes) are in
//! `remove_refusals.rs`. The stage files and the folders deleted outside Natamux
//! are in `remove_stage_files.rs`.

use std::fs;

use natamux_git::worktrees::{WorktreeState, worktree_state};
use natamux_proto::tasks::task::PullRequestStatus;

use super::*;

pub(super) fn path_of(task: &Task) -> PathBuf {
    PathBuf::from(task.workspace.clone().unwrap())
}

fn merged() -> PullRequestStatus {
    PullRequestStatus {
        state: "merged".into(),
        checks: "passing".into(),
    }
}

#[test]
fn removes_the_worktree_and_the_branch_when_it_is_merged() {
    let fx = Fx::new();
    let task = fx.task("Fix the bug");
    let created = fx.ws.create(&task.id, None).unwrap();

    let result = fx.ws.remove(&task.id, true).unwrap();

    assert!(result.branch_deleted);
    assert_eq!(result.task.workspace, None);
    assert_eq!(result.task.branch, None);
    assert!(!path_of(&created).exists());
    assert!(!fx.branch_exists(BRANCH));
    assert_eq!(
        fx.event_kinds(&task.id).last().unwrap(),
        "workspace.removed"
    );
}

#[test]
fn keeps_the_branch_when_asked_to() {
    let fx = Fx::new();
    let task = fx.task("Fix the bug");
    fx.ws.create(&task.id, None).unwrap();

    let result = fx.ws.remove(&task.id, false).unwrap();

    assert!(!result.branch_deleted);
    assert_eq!(result.task.branch.as_deref(), Some(BRANCH));
    assert!(fx.branch_exists(BRANCH));
}

#[test]
fn keeps_a_branch_that_has_commits_that_are_not_merged() {
    let fx = Fx::new();
    let task = fx.task("Fix the bug");
    let created = fx.ws.create(&task.id, None).unwrap();
    run_git(
        &path_of(&created),
        &["commit", "--allow-empty", "-m", "work"],
    );

    let result = fx.ws.remove(&task.id, true).unwrap();

    assert!(!result.branch_deleted);
    assert_eq!(result.task.workspace, None);
    assert_eq!(result.task.branch.as_deref(), Some(BRANCH));
    assert!(fx.branch_exists(BRANCH));
}

#[test]
fn deletes_the_branch_of_a_merged_pull_request_once_origin_has_all_of_it() {
    let fx = Fx::new();
    bare_origin(fx.root.path());
    let task = fx.task("Fix the bug");
    let created = fx.ws.create(&task.id, None).unwrap();
    let workspace = path_of(&created);
    run_git(&workspace, &["commit", "--allow-empty", "-m", "work"]);
    run_git(&workspace, &["push", "origin", BRANCH]);
    fx.tasks
        .record_pull_request_status(&task.id, merged())
        .unwrap();

    let result = fx.ws.remove(&task.id, true).unwrap();

    assert!(result.branch_deleted);
    assert!(!fx.branch_exists(BRANCH));
}

#[test]
fn keeps_the_branch_of_a_merged_pull_request_that_has_a_commit_origin_lacks() {
    let fx = Fx::new();
    bare_origin(fx.root.path());
    let task = fx.task("Fix the bug");
    let created = fx.ws.create(&task.id, None).unwrap();
    let workspace = path_of(&created);
    run_git(&workspace, &["commit", "--allow-empty", "-m", "work"]);
    run_git(&workspace, &["push", "origin", BRANCH]);
    run_git(
        &workspace,
        &["commit", "--allow-empty", "-m", "after the merge"],
    );
    fx.tasks
        .record_pull_request_status(&task.id, merged())
        .unwrap();

    let result = fx.ws.remove(&task.id, true).unwrap();

    assert!(!result.branch_deleted);
    assert!(fx.branch_exists(BRANCH));
}

#[test]
fn refuses_a_worktree_with_changes_not_committed_before_ending_its_shells() {
    let fx = Fx::new();
    let task = fx.task("Fix the bug");
    let created = fx.ws.create(&task.id, None).unwrap();
    let workspace = path_of(&created);
    fs::write(workspace.join("notes.txt"), "work in progress").unwrap();

    let refusal = fx.ws.remove(&task.id, true);
    assert!(is_conflict(&refusal));
    assert_eq!(
        error_text(refusal),
        format!(
            "{} has 1 change that is not committed. Commit or discard it first",
            task.id
        )
    );

    assert!(fx.closed_ids().is_empty());
    assert_eq!(
        fx.tasks.get_task(&task.id).unwrap().workspace,
        created.workspace
    );
    assert!(workspace.join("notes.txt").exists());

    run_git(&workspace, &["add", "notes.txt"]);
    run_git(&workspace, &["commit", "-m", "Notes"]);
    fx.ws.remove(&task.id, false).unwrap();
    assert_eq!(fx.closed_ids(), std::slice::from_ref(&task.id));
}

#[test]
fn makes_the_stage_of_the_agent_it_ends_wait_to_start_again() {
    let fx = Fx::new();
    let task = fx.task("Fix the bug");
    fx.ws.create(&task.id, None).unwrap();
    fx.start_agent(&task.id);

    let result = fx.ws.remove(&task.id, false).unwrap();

    assert_eq!(result.task.state, TaskState::Ready);
    assert_eq!(fx.state_of(&task.id), TaskState::Ready);
    let kinds = fx.event_kinds(&task.id);
    assert_eq!(
        kinds[kinds.len() - 2..],
        ["workspace.removed", "agent.stopped"]
    );
}

#[test]
fn also_stops_an_agent_that_ended_its_turn_while_the_worktree_was_removed() {
    let fx = Fx::new();
    let task = fx.task("Fix the bug");
    fx.ws.create(&task.id, None).unwrap();
    fx.start_agent(&task.id);
    let tasks = fx.tasks.clone();
    // The agent's Stop hook as its shell ends.
    let ending = fx.build(|options| {
        options.close_terminals = Arc::new(move |id| {
            let event = json!({ "source": "agent", "type": "agent.finished" });
            tasks.apply_event(id, Some(&event)).unwrap();
        });
    });

    let result = ending.remove(&task.id, false).unwrap();

    assert_eq!(result.task.state, TaskState::Ready);
}

#[test]
fn makes_a_stage_whose_agent_waited_for_you_wait_to_start_again() {
    let fx = Fx::new();
    let task = fx.task("Fix the bug");
    fx.ws.create(&task.id, None).unwrap();
    fx.start_agent(&task.id);
    fx.event(&task.id, "agent", "agent.asked");

    let result = fx.ws.remove(&task.id, false).unwrap();

    assert_eq!(result.task.state, TaskState::Ready);
}

// Failed, blocked and done do not depend on a live agent: the task keeps them.
fn keeps_a_task_that_is(to: &str, state: TaskState) {
    let fx = Fx::new();
    let task = fx.task("Fix the bug");
    fx.ws.create(&task.id, None).unwrap();
    fx.override_state(&task.id, to);

    let result = fx.ws.remove(&task.id, false).unwrap();

    assert_eq!(result.task.state, state);
    assert_eq!(
        fx.event_kinds(&task.id).last().unwrap(),
        "workspace.removed"
    );
}

#[test]
fn keeps_a_task_that_is_failed() {
    keeps_a_task_that_is("failed", TaskState::Failed);
}

#[test]
fn keeps_a_task_that_is_blocked() {
    keeps_a_task_that_is("blocked", TaskState::Blocked);
}

#[test]
fn keeps_a_task_that_is_done() {
    keeps_a_task_that_is("done", TaskState::Done);
}

#[test]
fn keeps_the_state_of_a_running_task_when_the_removal_is_refused() {
    let fx = Fx::new();
    let task = fx.task("Fix the bug");
    let created = fx.ws.create(&task.id, None).unwrap();
    fx.start_agent(&task.id);
    fs::write(path_of(&created).join("notes.txt"), "work in progress").unwrap();

    assert!(is_conflict(&fx.ws.remove(&task.id, false)));

    let now = fx.tasks.get_task(&task.id).unwrap();
    assert_eq!(now.state, TaskState::Running);
    assert_eq!(now.workspace, created.workspace);
}

#[test]
fn refuses_a_worktree_whose_status_git_cannot_read_before_ending_its_shells() {
    let fx = Fx::new();
    let task = fx.task("Fix the bug");
    let created = fx.ws.create(&task.id, None).unwrap();
    let workspace = path_of(&created);
    // Windows marks .git hidden, and a hidden file cannot be opened for writing.
    fs::remove_file(workspace.join(".git")).unwrap();
    fs::write(workspace.join(".git"), "not a git file\n").unwrap();

    assert!(is_git(&fx.ws.remove(&task.id, false)));

    assert!(fx.closed_ids().is_empty());
    assert_eq!(
        fx.tasks.get_task(&task.id).unwrap().workspace,
        created.workspace
    );
}

#[test]
fn stops_the_agent_when_git_refuses_the_worktree_after_its_shells_ended() {
    let fx = Fx::new();
    let task = fx.task("Fix the bug");
    let created = fx.ws.create(&task.id, None).unwrap();
    fx.start_agent(&task.id);
    run_git(
        &fx.repo,
        &["worktree", "lock", path_text(&path_of(&created))],
    );

    assert!(error_text(fx.ws.remove(&task.id, false)).contains("locked"));

    assert_eq!(fx.closed_ids(), std::slice::from_ref(&task.id));
    let now = fx.tasks.get_task(&task.id).unwrap();
    assert_eq!(now.state, TaskState::Ready);
    assert_eq!(now.workspace, created.workspace);
}

#[test]
fn removes_a_clean_worktree_with_a_submodule_checked_out() {
    let fx = Fx::new();
    let lib = fx.root.path().join("lib");
    fs::create_dir(&lib).unwrap();
    run_git(&lib, &["init", "-b", "main"]);
    run_git(&lib, &["commit", "--allow-empty", "-m", "lib"]);
    let task = fx.task("Fix the bug");
    let path = path_of(&fx.ws.create(&task.id, None).unwrap());
    let add = [
        "-c",
        "protocol.file.allow=always",
        "submodule",
        "add",
        path_text(&lib),
        "lib",
    ];
    run_git(&path, &add);
    run_git(&path, &["commit", "-m", "Add lib"]);

    let result = fx.ws.remove(&task.id, false).unwrap();

    assert_eq!(result.task.workspace, None);
    assert!(!path.exists());
}

#[test]
fn removes_a_workspace_made_again_the_same_way() {
    let fx = Fx::new();
    let task = fx.task("Fix the bug");
    fx.ws.create(&task.id, None).unwrap();
    fx.start_agent(&task.id);
    fx.ws.remove(&task.id, false).unwrap();

    let again = fx.ws.create(&task.id, None).unwrap();
    assert_eq!(again.state, TaskState::Ready);
    assert_eq!(
        again.workspace,
        Some(path_text(&fx.ws.worktree_path_for(&task)).to_string())
    );
    fx.event(&task.id, "agent", "agent.started");

    let result = fx.ws.remove(&task.id, false).unwrap();

    assert_eq!(result.task.state, TaskState::Ready);
    assert_eq!(result.task.workspace, None);
    let stopped = fx
        .event_kinds(&task.id)
        .iter()
        .filter(|kind| *kind == "agent.stopped")
        .count();
    assert_eq!(stopped, 2);
}

#[test]
fn keeps_a_stage_that_waits_for_approval() {
    let fx = Fx::new();
    let task = fx.task("Fix the bug");
    fx.ws.create(&task.id, None).unwrap();
    fx.override_state(&task.id, "review");

    let result = fx.ws.remove(&task.id, false).unwrap();

    assert_eq!(result.task.state, TaskState::Review);
    assert_eq!(
        fx.event_kinds(&task.id).last().unwrap(),
        "workspace.removed"
    );
}

#[test]
fn ends_the_shells_once_and_drops_the_git_entry_of_a_worktree_that_is_there() {
    let fx = Fx::new();
    let task = fx.task("Fix the bug");
    let path = path_of(&fx.ws.create(&task.id, None).unwrap());

    let result = fx.ws.remove(&task.id, false).unwrap();

    assert_eq!(fx.closed_ids(), std::slice::from_ref(&task.id));
    assert!(!path.exists());
    assert_eq!(
        worktree_state(&fx.repo, &path).unwrap(),
        WorktreeState::Missing
    );
    assert_eq!(result.task.workspace, None);
    assert_eq!(
        fx.event_kinds(&task.id).last().unwrap(),
        "workspace.removed"
    );
}

#[test]
fn removes_a_worktree_that_has_ignored_files_only() {
    let fx = Fx::new();
    let task = fx.task("Fix the bug");
    let path = path_of(&fx.ws.create(&task.id, None).unwrap());
    fs::write(path.join(".gitignore"), "*.log\n").unwrap();
    run_git(&path, &["add", ".gitignore"]);
    run_git(&path, &["commit", "-m", "ignore logs"]);
    fs::write(path.join("debug.log"), "noise").unwrap();

    let result = fx.ws.remove(&task.id, false).unwrap();

    assert_eq!(result.task.workspace, None);
    assert!(!path.exists());
}

#[test]
fn passes_on_a_git_failure_for_a_locked_worktree_and_leaves_the_task_as_it_was() {
    let fx = Fx::new();
    let task = fx.task("Fix the bug");
    let path = path_of(&fx.ws.create(&task.id, None).unwrap());
    run_git(&fx.repo, &["worktree", "lock", path_text(&path)]);

    let text = error_text(fx.ws.remove(&task.id, false));

    assert!(text.contains("git worktree failed: "));
    assert!(text.contains("locked"));
    assert!(path.exists());
    assert_eq!(
        fx.workspace_of(&task.id),
        Some(path_text(&path).to_string())
    );
    assert!(
        !fx.event_kinds(&task.id)
            .contains(&"workspace.removed".to_string())
    );
}

#[test]
fn lets_the_task_get_a_new_workspace_after_the_old_one_is_removed() {
    let fx = Fx::new();
    let task = fx.task("Fix the bug");
    fx.ws.create(&task.id, None).unwrap();
    fx.ws.remove(&task.id, true).unwrap();

    let again = fx.ws.create(&task.id, None).unwrap();

    assert!(again.workspace.is_some());
}

#[test]
fn goes_on_with_the_branch_that_was_kept_when_the_task_gets_a_new_workspace() {
    let fx = Fx::new();
    let task = fx.task("Fix the bug");
    let created = fx.ws.create(&task.id, None).unwrap();
    run_git(
        &path_of(&created),
        &["commit", "--allow-empty", "-m", "work"],
    );
    fx.ws.remove(&task.id, true).unwrap();

    let again = fx.ws.create(&task.id, None).unwrap();

    assert_eq!(again.branch.as_deref(), Some(BRANCH));
    assert_eq!(
        run_git(&path_of(&again), &["log", "-1", "--format=%s"]).trim(),
        "work"
    );
}
