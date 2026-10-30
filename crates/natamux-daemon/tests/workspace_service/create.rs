//! `describe('create')`, with its `steps` group.

use std::fs;
use std::sync::{Barrier, OnceLock};

use natamux_daemon::tasks::errors::TaskError;
use natamux_proto::workspaces::workspace::DiffStat;

use super::*;

#[test]
fn adds_a_worktree_and_records_it_on_the_task() {
    let fx = Fx::new();
    let task = fx.task("Fix the bug");

    let updated = fx.ws.create(&task.id, None, None).unwrap();

    let expected = fx.worktrees.join("T-0001");
    assert_eq!(updated.workspace, Some(path_text(&expected).to_string()));
    assert_eq!(updated.branch.as_deref(), Some(BRANCH));
    assert!(expected.join(".git").exists());
    assert!(fx.branch_exists(BRANCH));
    assert_eq!(fx.tasks.get_task(&task.id).unwrap(), updated);
}

#[test]
fn adds_the_worktree_in_the_projects_own_worktree_folder() {
    let fx = Fx::new();
    let other = fx.root.path().join("other");
    fs::create_dir(&other).unwrap();
    run_git(&other, &["init", "-b", "main"]);
    run_git(&other, &["commit", "--allow-empty", "-m", "first"]);
    let trees = fx.root.path().join("other-trees");
    let repo_data = fx.root.path().join("repo-data");
    fs::create_dir(&repo_data).unwrap();
    let projects = json!([
        { "id": "default", "name": "default", "path": fx.repo },
        { "id": "other", "name": "other", "path": other, "worktreesDir": trees },
    ]);
    fs::write(repo_data.join("repos.json"), projects.to_string()).unwrap();
    let repos = Repos::new(&repo_data, clock_of(&fx.clock));
    let with_repos = fx.build(|options| options.repos = repos);
    let there = fx.task_with(json!({ "title": "Fix it there", "repo": "other" }));
    let here = fx.task("Fix it here");

    let there_made = with_repos.create(&there.id, None, None).unwrap();
    let here_made = with_repos.create(&here.id, None, None).unwrap();

    assert_eq!(
        there_made.workspace,
        Some(path_text(&trees.join(&there.id)).to_string())
    );
    let here_path = fx.worktrees.join(&here.id);
    assert_eq!(here_made.workspace, Some(path_text(&here_path).to_string()));
}

#[test]
fn logs_a_workspace_created_event_and_publishes_the_change() {
    let fx = Fx::new();
    let task = fx.task("Fix the bug");

    fx.ws.create(&task.id, None, None).unwrap();

    assert_eq!(
        fx.event_kinds(&task.id),
        ["task.created", "workspace.created"]
    );
    let last = fx.task_messages.lock().unwrap().last().cloned().unwrap();
    match last {
        DaemonMessage::TaskUpdated { accepted, .. } => assert!(accepted),
        other => panic!("expected task.updated, got {}", other.kind()),
    }
}

#[test]
fn does_not_change_the_state_of_the_task() {
    let fx = Fx::new();
    let task = fx.task("Fix the bug");
    let updated = fx.ws.create(&task.id, None, None).unwrap();
    assert_eq!(updated.state, TaskState::Draft);
}

#[test]
fn refuses_a_second_workspace_for_the_same_task() {
    let fx = Fx::new();
    let task = fx.task("Fix the bug");
    fx.ws.create(&task.id, None, None).unwrap();

    assert!(is_conflict(&fx.ws.create(&task.id, None, None)));
}

#[test]
fn refuses_a_task_that_is_done_or_cancelled() {
    let fx = Fx::new();
    let task = fx.task("Fix the bug");
    fx.event(&task.id, "human", "task.cancelled");

    assert!(error_text(fx.ws.create(&task.id, None, None)).contains("cancelled"));
}

#[test]
fn throws_task_not_found_error_for_an_unknown_task() {
    let fx = Fx::new();
    let result = fx.ws.create("T-0099", None, None);
    assert!(matches!(
        result,
        Err(WorkspaceError::Task(TaskError::NotFound(_)))
    ));
}

#[test]
fn lets_only_one_of_two_requests_at_the_same_time_through() {
    let fx = Fx::new();
    let task = fx.task("Fix the bug");
    let barrier = Arc::new(Barrier::new(2));
    let runs: Vec<_> = (0..2)
        .map(|_| {
            let (ws, id, barrier) = (Arc::clone(&fx.ws), task.id.clone(), Arc::clone(&barrier));
            std::thread::spawn(move || {
                barrier.wait();
                ws.create(&id, None, None)
            })
        })
        .collect();
    let results: Vec<_> = runs.into_iter().map(|run| run.join().unwrap()).collect();

    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    let failure = results.iter().find(|result| result.is_err()).unwrap();
    assert!(is_conflict(failure));
}

// The main checkout was not pulled: a new task starts from its remote branch, fetched now.
#[test]
fn starts_a_new_workspace_from_the_newest_remote_branch_when_the_checkout_is_behind() {
    let fx = Fx::new();
    let origin = bare_origin(fx.root.path());
    run_git(&fx.repo, &["push", "-u", "origin", "main"]);
    // Someone else pushes a commit, and the checkout does not pull it.
    let other = fx.root.path().join("other");
    run_git(
        fx.root.path(),
        &["clone", path_text(&origin), path_text(&other)],
    );
    fs::write(other.join("newer.txt"), "newer\n").unwrap();
    run_git(&other, &["add", "newer.txt"]);
    run_git(&other, &["commit", "-m", "newer"]);
    run_git(&other, &["push", "origin", "main"]);
    let newer = run_git(&other, &["rev-parse", "HEAD"]);
    let before = run_git(&fx.repo, &["rev-parse", "HEAD"]);

    let created = fx.with_workspace("Fresh");

    let workspace = PathBuf::from(created.workspace.clone().unwrap());
    assert_eq!(
        run_git(&workspace, &["rev-parse", "HEAD"]).trim(),
        newer.trim()
    );
    assert_eq!(
        run_git(&fx.repo, &["rev-parse", "HEAD"]).trim(),
        before.trim()
    );
    // The task branch follows no branch, so a plain push never goes to main.
    assert!(!git_ok(&workspace, &["rev-parse", "@{upstream}"]));
    // The diff counts the task's own work, not the commits the checkout did not pull.
    let stats = fx.ws.diff_stats().unwrap();
    let own = stats
        .iter()
        .find(|(id, _)| *id == created.id)
        .map(|(_, stat)| *stat);
    assert_eq!(own, Some(DiffStat::default()));
}

#[test]
fn keeps_local_commits_that_are_not_pushed_and_works_without_a_remote_or_a_network() {
    let fx = Fx::new();
    // No remote: the checkout's HEAD, as before.
    let head = run_git(&fx.repo, &["rev-parse", "HEAD"]);
    let first = fx.with_workspace("No remote");
    let first_path = PathBuf::from(first.workspace.unwrap());
    assert_eq!(
        run_git(&first_path, &["rev-parse", "HEAD"]).trim(),
        head.trim()
    );

    // A remote that is gone, and a local commit ahead of what was pushed.
    let origin = bare_origin(fx.root.path());
    run_git(&fx.repo, &["push", "-u", "origin", "main"]);
    run_git(&fx.repo, &["commit", "--allow-empty", "-m", "local only"]);
    let local = run_git(&fx.repo, &["rev-parse", "HEAD"]);
    fs::remove_dir_all(&origin).unwrap();

    let second = fx.with_workspace("Offline");
    let second_path = PathBuf::from(second.workspace.unwrap());
    assert_eq!(
        run_git(&second_path, &["rev-parse", "HEAD"]).trim(),
        local.trim()
    );
}

#[test]
fn leaves_the_task_as_it_was_when_git_fails() {
    let fx = Fx::new();
    let task = fx.task("Fix the bug");
    run_git(&fx.repo, &["branch", BRANCH]);

    assert!(is_git(&fx.ws.create(&task.id, None, None)));

    assert_eq!(fx.workspace_of(&task.id), None);
}

// What a dashboard would read on each `workspace.preparing`: the step, the error, and what
// `preparing()` says at that moment.
type Step = (Option<String>, Option<String>, Vec<(String, String)>);

fn step(name: Option<&str>, error: Option<&str>, preparing: &[(&str, &str)]) -> Step {
    let preparing = preparing
        .iter()
        .map(|(id, at)| (id.to_string(), at.to_string()))
        .collect();
    (
        name.map(str::to_string),
        error.map(str::to_string),
        preparing,
    )
}

fn watched(fx: &Fx) -> (Arc<WorkspaceService>, Arc<Mutex<Vec<Step>>>) {
    let bus = EventBus::new();
    let steps: Arc<Mutex<Vec<Step>>> = Arc::default();
    let cell: Arc<OnceLock<Arc<WorkspaceService>>> = Arc::default();
    let (seen, service) = (Arc::clone(&steps), Arc::clone(&cell));
    bus.subscribe(move |message| {
        let DaemonMessage::WorkspacePreparing { step, error, .. } = message else {
            return;
        };
        let preparing = service.get().map(|ws| ws.preparing()).unwrap_or_default();
        let preparing = preparing
            .into_iter()
            .map(|(id, at)| (id, at.to_string()))
            .collect();
        seen.lock()
            .unwrap()
            .push((step.clone(), error.clone(), preparing));
    });
    let ws = fx.build(|options| options.bus = bus);
    cell.set(Arc::clone(&ws)).ok();
    (ws, steps)
}

#[test]
fn steps_tells_each_step_of_a_new_workspace_then_that_it_ended_and_logs_the_times() {
    let fx = Fx::new();
    let (watched, steps) = watched(&fx);
    let task = fx.task("Fix the bug");

    watched.create(&task.id, None, None).unwrap();

    assert_eq!(
        *steps.lock().unwrap(),
        [
            step(Some("fetch"), None, &[("T-0001", "fetch")]),
            step(Some("checkout"), None, &[("T-0001", "checkout")]),
            step(None, None, &[]),
        ]
    );
    assert!(
        fx.log_lines()
            .contains(&"T-0001 workspace: fetch 0 ms, checkout 0 ms, ok".to_string())
    );
}

#[test]
fn steps_tells_the_error_when_git_fails() {
    let fx = Fx::new();
    let (watched, steps) = watched(&fx);
    let task = fx.task("Fix the bug");
    run_git(&fx.repo, &["branch", BRANCH]);

    assert!(is_git(&watched.create(&task.id, None, None)));

    let (name, error, preparing) = steps.lock().unwrap().last().cloned().unwrap();
    assert_eq!(name, None);
    assert!(error.unwrap().contains("git worktree failed"));
    assert!(preparing.is_empty());
    assert!(
        fx.log_lines()
            .contains(&"T-0001 workspace: fetch 0 ms, checkout 0 ms, failed".to_string())
    );
}

// Rust's `create` blocks, so the check that Node made right after the call (`preparing()` before
// the branches are read) is the `preparing` of the first message: it is sent before git runs.
#[test]
fn steps_is_preparing_before_it_reads_the_branches_and_tells_a_failure_there_too() {
    let fx = Fx::new();
    let (watched, steps) = watched(&fx);
    let task = fx.task("Fix the bug");

    assert!(is_conflict(
        &watched.create(&task.id, Some("no-such-branch"), None)
    ));

    assert_eq!(
        *steps.lock().unwrap(),
        [
            step(Some("fetch"), None, &[("T-0001", "fetch")]),
            step(
                None,
                Some("There is no local branch named no-such-branch"),
                &[]
            ),
        ]
    );
    assert!(
        fx.log_lines()
            .contains(&"T-0001 workspace: failed".to_string())
    );
}

#[test]
fn steps_tells_nothing_for_a_task_that_cannot_get_a_workspace_at_all() {
    let fx = Fx::new();
    let (watched, steps) = watched(&fx);
    let task = fx.task("Fix the bug");
    fx.event(&task.id, "human", "task.cancelled");

    assert!(is_conflict(&watched.create(&task.id, None, None)));

    assert!(steps.lock().unwrap().is_empty());
    assert!(fx.log_lines().is_empty());
}
