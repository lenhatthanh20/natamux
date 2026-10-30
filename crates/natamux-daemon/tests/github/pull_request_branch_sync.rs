//! The finder looks for the pull request of the branch the agent switched the worktree to: the
//! real `sync_branch` on a real worktree, gh faked.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use natamux_daemon::github::pull_request_watcher::{
    PullRequestWatcher, PullRequestWatcherOptions, SyncBranch,
};
use natamux_daemon::tasks::records::WorkspaceChange;
use natamux_daemon::workspaces::branch_sync::sync_branch;
use natamux_git::gh::RunGh;
use natamux_git::test_support::{TempDir, run_git, test_repo};
use natamux_proto::tasks::task::Task;
use serde_json::json;

use super::pull_request_watcher::{Found, github, on_origin, options};
use super::{Fx, values_of};

const OLD: &str = "natamux/t-1-fix-it";
const SWITCHED: &str = "feature-x";
const URL: &str = "https://github.com/o/r/pull/9";

/// A task with a real worktree, made on `OLD`, whose agent then switched it to `SWITCHED`.
fn switched_task(fx: &Fx) -> (TempDir, PathBuf, Task) {
    let (root, repo) = test_repo();
    let worktree = root.path().join("T-1");
    let path = worktree.to_str().unwrap();
    run_git(&repo, &["worktree", "add", "-b", OLD, path]);
    run_git(&worktree, &["switch", "-c", SWITCHED]);
    let input = json!({ "title": "Fix it" });
    let task = fx.tasks.create_task(Some(&input), &[], true).unwrap();
    let change = WorkspaceChange {
        workspace: Some(path.to_string()),
        branch: Some(OLD.into()),
    };
    let task = fx.tasks.record_workspace(&task.id, change).unwrap();
    (root, repo, task)
}

fn syncing(fx: &Fx, repo: &Path) -> SyncBranch {
    let (tasks, repo) = (fx.tasks.clone(), repo.to_string_lossy().into_owned());
    Arc::new(move |task_id: &str| sync_branch(&tasks, &|_| Ok(repo.clone()), task_id))
}

fn finding(fx: &Fx, gh: RunGh, repo: &Path) -> PullRequestWatcherOptions {
    let mut options = options(fx, gh);
    options.has_remote_branch = Some(on_origin());
    options.sync_branch = syncing(fx, repo);
    options
}

fn switched_pr() -> Vec<Found> {
    vec![Found {
        branch: SWITCHED,
        url: URL,
        state: "OPEN",
    }]
}

fn branch_changes(fx: &Fx, task_id: &str) -> usize {
    let events = fx.tasks.list_events(task_id).unwrap();
    events
        .iter()
        .filter(|stored| stored.event.kind == "workspace.branch-changed")
        .count()
}

#[test]
fn links_the_pull_request_of_the_branch_the_agent_switched_to() {
    let fx = Fx::new();
    let (_root, repo, task) = switched_task(&fx);
    let (gh, _) = github(switched_pr());

    PullRequestWatcher::new(finding(&fx, gh, &repo)).check_all();

    let linked = fx.tasks.get_task(&task.id).unwrap();
    assert_eq!(linked.branch.as_deref(), Some(SWITCHED));
    assert_eq!(linked.pr.as_deref(), Some(URL));
    assert_eq!(branch_changes(&fx, &task.id), 1);
}

#[test]
fn a_check_of_one_task_asks_for_the_branch_the_agent_switched_to() {
    let fx = Fx::new();
    let (_root, repo, task) = switched_task(&fx);
    let (gh, calls) = github(switched_pr());

    let checked = PullRequestWatcher::new(finding(&fx, gh, &repo))
        .check(&task.id)
        .unwrap();

    assert_eq!(checked.branch.as_deref(), Some(SWITCHED));
    assert_eq!(checked.pr.as_deref(), Some(URL));
    let heads: Vec<String> = calls
        .lock()
        .unwrap()
        .iter()
        .flat_map(|call| values_of(call, "--head"))
        .collect();
    assert_eq!(heads, [SWITCHED]);
    assert_eq!(branch_changes(&fx, &task.id), 1);
}
