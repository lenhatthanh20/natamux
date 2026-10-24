//! `pull-request-watcher.test.ts`. The tests of the timer move the manual `Scheduler` on with
//! `tick`, then `settle` the watcher: gh runs on the worker thread, not in the timer's job.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use natamux_daemon::github::pull_request_watcher::{
    CwdOf, HasRemoteBranch, Log, PullRequestWatcher, PullRequestWatcherOptions,
    SETTLED_INTERVAL_MS, WATCH_INTERVAL_MS, is_watched,
};
use natamux_daemon::projects::repos::Repos;
use natamux_daemon::shared::event_bus::EventBus;
use natamux_daemon::shared::jobs::system_clock;
use natamux_daemon::tasks::records::WorkspaceChange;
use natamux_git::gh::{GitHubError, RunGh};
use natamux_proto::tasks::task::{PullRequestStatus, Task, TaskState};
use serde_json::{Map, Value, json};

use super::{Calls, Fx, GhCall, recording, values_of};

pub(super) fn status(state: &str, checks: &str) -> PullRequestStatus {
    PullRequestStatus {
        state: state.into(),
        checks: checks.into(),
    }
}

pub(super) fn task_with_pr(fx: &Fx, number: u32, repo: Option<&str>) -> Task {
    let task = fx
        .tasks
        .create_task(
            Some(&json!({ "title": format!("Task {number}"), "repo": repo })),
            &[],
            true,
        )
        .unwrap();
    let url = format!("https://github.com/o/r/pull/{number}");
    fx.tasks
        .record_git(&task.id, "pr.opened", json!({ "url": url }))
        .unwrap()
}

/// A task with a workspace on `branch`, and no pull request yet.
pub(super) fn task_on_branch(fx: &Fx, branch: &str, repo: Option<&str>) -> Task {
    let task = fx
        .tasks
        .create_task(Some(&json!({ "title": branch, "repo": repo })), &[], true)
        .unwrap();
    let change = WorkspaceChange {
        workspace: Some(format!("/work/{branch}")),
        branch: Some(branch.into()),
    };
    fx.tasks.record_workspace(&task.id, change).unwrap()
}

/// Answers like `gh api graphql`, with each pull request asked for in its given state. A number
/// without a state is not found.
pub(super) fn answer_graphql(states: &[(u64, &str)], args: &[String]) -> String {
    let query = args
        .iter()
        .find(|arg| arg.starts_with("query="))
        .cloned()
        .unwrap_or_default();
    let mut repository = Map::new();
    for rest in query.split("pullRequest(number: ").skip(1) {
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        let number: u64 = digits.parse().unwrap();
        let node = states.iter().find(|(known, _)| *known == number).map_or(
            Value::Null,
            |(_, state)| json!({ "state": state, "isDraft": false, "commits": { "nodes": [] } }),
        );
        repository.insert(format!("pr{number}"), node);
    }
    json!({ "data": { "repository": repository } }).to_string()
}

pub(super) fn graphql(states: Vec<(u64, &'static str)>) -> (RunGh, Calls) {
    recording(move |call| Ok(answer_graphql(&states, &call.args)))
}

/// A pull request of a branch, for the stub of `github`.
pub(super) struct Found {
    pub branch: &'static str,
    pub url: &'static str,
    pub state: &'static str,
}

/// Answers `gh pr list --head` with the pull request of that branch, `gh pr list` without it with
/// the pull request of each branch, and `gh pr view` and `gh api graphql` with the state of that
/// pull request.
pub(super) fn github_with(
    prs: Vec<Found>,
    wrap: impl Fn(&GhCall) -> Option<Result<String, GitHubError>> + Send + Sync + 'static,
) -> (RunGh, Calls) {
    let numbers: Vec<(u64, &str)> = prs
        .iter()
        .map(|pr| {
            (
                pr.url.rsplit('/').next().unwrap().parse().unwrap(),
                pr.state,
            )
        })
        .collect();
    recording(move |call| {
        if let Some(answer) = wrap(call) {
            return answer;
        }
        let args = &call.args;
        let listed = |pr: &Found| json!({ "url": pr.url, "state": pr.state });
        if args[0] == "pr" && args[1] == "list" {
            if !args.contains(&"--head".to_string()) {
                let all: Vec<Value> = prs
                    .iter()
                    .map(|pr| json!({ "url": pr.url, "state": pr.state, "headRefName": pr.branch }))
                    .collect();
                return Ok(Value::Array(all).to_string());
            }
            let head = values_of(call, "--head").remove(0);
            let found: Vec<Value> = prs
                .iter()
                .filter(|pr| pr.branch == head)
                .map(listed)
                .collect();
            return Ok(Value::Array(found).to_string());
        }
        if args[0] == "pr" && args[1] == "view" {
            let state = prs
                .iter()
                .find(|pr| pr.url == args[2])
                .map_or("OPEN", |pr| pr.state);
            return Ok(
                json!({ "state": state, "isDraft": false, "statusCheckRollup": [] }).to_string(),
            );
        }
        Ok(answer_graphql(&numbers, args))
    })
}

pub(super) fn github(prs: Vec<Found>) -> (RunGh, Calls) {
    github_with(prs, |_| None)
}

pub(super) fn fails(text: &'static str) -> (RunGh, Arc<AtomicUsize>) {
    let count = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&count);
    let gh: RunGh = Arc::new(move |_, _| {
        counter.fetch_add(1, Ordering::SeqCst);
        Err(GitHubError::new(text))
    });
    (gh, count)
}

pub(super) fn bus_with_client() -> Arc<EventBus> {
    let bus = EventBus::new();
    bus.subscribe_client(|_| {});
    bus
}

pub(super) fn options(fx: &Fx, gh: RunGh) -> PullRequestWatcherOptions {
    let cwd_of: CwdOf = Arc::new(|_| "/repo".to_string());
    let log: Log = Arc::new(|_| {});
    let mut options =
        PullRequestWatcherOptions::new(fx.tasks.clone(), cwd_of, gh, Arc::clone(&fx.scheduler));
    options.log = Some(log);
    options
}

pub(super) fn on_origin() -> HasRemoteBranch {
    Arc::new(|_, _| Ok(true))
}

/// Origin never has the branch; each look is counted.
pub(super) fn counting_no_branch(looks: &Arc<AtomicUsize>) -> HasRemoteBranch {
    let looks = Arc::clone(looks);
    Arc::new(move |_, _| {
        looks.fetch_add(1, Ordering::SeqCst);
        Ok(false)
    })
}

pub(super) fn logged(options: &mut PullRequestWatcherOptions) -> Arc<Mutex<Vec<String>>> {
    let lines: Arc<Mutex<Vec<String>>> = Arc::default();
    let sink = Arc::clone(&lines);
    let log: Log = Arc::new(move |line: &str| sink.lock().unwrap().push(line.to_string()));
    options.log = Some(log);
    lines
}

/// `advanceTimersByTimeAsync`: the clock moves on, then the worker finishes what it was given.
pub(super) fn tick(fx: &Fx, watcher: &PullRequestWatcher, ms: u64) {
    fx.scheduler.advance(ms);
    watcher.settle();
}

pub(super) fn started(options: PullRequestWatcherOptions) -> PullRequestWatcher {
    let watcher = PullRequestWatcher::new(options);
    watcher.start();
    watcher.settle();
    watcher
}

pub(super) fn count(calls: &Calls) -> usize {
    calls.lock().unwrap().len()
}

pub(super) fn pr_events(fx: &Fx, id: &str) -> Vec<Map<String, Value>> {
    fx.tasks
        .list_events(id)
        .unwrap()
        .into_iter()
        .filter(|stored| stored.event.kind == "pr.opened")
        .map(|stored| stored.event.payload)
        .collect()
}

#[test]
fn watches_a_task_with_a_pull_request_that_is_not_merged_or_closed() {
    let fx = Fx::new();
    let task = task_with_pr(&fx, 1, None);
    let with = |status: Option<PullRequestStatus>| Task {
        pr_status: status,
        ..task.clone()
    };

    assert!(is_watched(&task));
    assert!(is_watched(&with(Some(status("draft", "none")))));
    assert!(!is_watched(&with(Some(status("merged", "none")))));
    assert!(!is_watched(&with(Some(status("closed", "none")))));
    let cancelled = Task {
        state: TaskState::Cancelled,
        ..task.clone()
    };
    assert!(!is_watched(&cancelled));
    assert!(!is_watched(&Task {
        pr: None,
        ..task.clone()
    }));
}

#[test]
fn reads_the_status_of_every_watched_pull_request() {
    let fx = Fx::new();
    let first = task_with_pr(&fx, 1, None);
    let second = task_with_pr(&fx, 2, None);
    fx.tasks
        .create_task(Some(&json!({ "title": "No pull request" })), &[], true)
        .unwrap();
    let (gh, calls) = graphql(vec![(1, "OPEN"), (2, "MERGED")]);

    PullRequestWatcher::new(options(&fx, gh)).check_all();

    assert_eq!(count(&calls), 1);
    let status_of = |id: &str| fx.tasks.get_task(id).unwrap().pr_status;
    assert_eq!(status_of(&first.id), Some(status("open", "none")));
    assert_eq!(status_of(&second.id), Some(status("merged", "none")));
}

#[test]
fn asks_gh_once_per_github_repository_in_the_folder_of_one_of_its_tasks() {
    let fx = Fx::new();
    task_with_pr(&fx, 1, None);
    task_with_pr(&fx, 2, None);
    let web = fx
        .tasks
        .create_task(Some(&json!({ "title": "Web", "repo": "web" })), &[], true)
        .unwrap();
    let url = "https://github.com/o/web/pull/3";
    fx.tasks
        .record_git(&web.id, "pr.opened", json!({ "url": url }))
        .unwrap();
    let (gh, calls) = graphql(vec![]);
    let mut options = options(&fx, gh);
    let cwd_of: CwdOf = Arc::new(|repo: Option<&str>| match repo {
        None => "/default".to_string(),
        Some(repo) => format!("/{repo}"),
    });
    options.cwd_of = cwd_of;

    PullRequestWatcher::new(options).check_all();

    let calls = calls.lock().unwrap();
    let by_name = |name: &str| {
        calls
            .iter()
            .find(|call| values_of(call, "-f").last().map(String::as_str) == Some(name))
            .unwrap()
    };
    assert_eq!(calls.len(), 2);
    assert_eq!(by_name("name=r").cwd, "/default");
    assert_eq!(by_name("name=web").cwd, "/web");
    assert!(values_of(by_name("name=r"), "-f").contains(&"owner=o".to_string()));
}

#[test]
fn runs_gh_in_the_data_folder_for_a_removed_project() {
    let fx = Fx::new();
    let task = fx
        .tasks
        .create_task(Some(&json!({ "title": "Gone", "repo": "gone" })), &[], true)
        .unwrap();
    let url = "https://github.com/o/r/pull/4";
    fx.tasks
        .record_git(&task.id, "pr.opened", json!({ "url": url }))
        .unwrap();
    let repos = Repos::new(&fx.dir.0, system_clock());
    let (gh, calls) = graphql(vec![(4, "MERGED")]);
    let mut options = options(&fx, gh);
    let data = fx.dir.0.to_string_lossy().into_owned();
    let cwd_of: CwdOf = Arc::new(move |repo| repos.path_of(repo).unwrap_or_else(|_| data.clone()));
    options.cwd_of = cwd_of;

    PullRequestWatcher::new(options).check_all();

    let cwds: Vec<String> = calls
        .lock()
        .unwrap()
        .iter()
        .map(|c| c.cwd.clone())
        .collect();
    assert_eq!(cwds, [fx.dir.0.to_string_lossy().into_owned()]);
    let state = fx
        .tasks
        .get_task(&task.id)
        .unwrap()
        .pr_status
        .map(|s| s.state);
    assert_eq!(state.as_deref(), Some("merged"));
}

#[test]
fn leaves_a_pull_request_that_github_did_not_find_and_reads_the_others() {
    let fx = Fx::new();
    let found = task_with_pr(&fx, 1, None);
    let missing = task_with_pr(&fx, 2, None);
    let (gh, _) = graphql(vec![(1, "OPEN")]);

    PullRequestWatcher::new(options(&fx, gh)).check_all();

    let state = fx
        .tasks
        .get_task(&found.id)
        .unwrap()
        .pr_status
        .map(|s| s.state);
    assert_eq!(state.as_deref(), Some("open"));
    assert_eq!(fx.tasks.get_task(&missing.id).unwrap().pr_status, None);
}

#[test]
fn goes_on_after_a_failure_and_reports_the_same_failure_once() {
    let fx = Fx::new();
    task_with_pr(&fx, 1, None);
    task_with_pr(&fx, 2, None);
    let (gh, _) = fails("gh is not installed or not on the PATH");
    let mut options = options(&fx, gh);
    let lines = logged(&mut options);
    let watcher = PullRequestWatcher::new(options);

    watcher.check_all();
    watcher.check_all();

    assert_eq!(
        *lines.lock().unwrap(),
        ["Pull request check: GitHub CLI failed: gh is not installed or not on the PATH"]
    );
}

#[test]
fn asks_nothing_while_no_dashboard_listens_and_once_when_the_first_one_comes() {
    let fx = Fx::new();
    task_with_pr(&fx, 1, None);
    let (gh, calls) = graphql(vec![(1, "OPEN")]);
    let bus = EventBus::new();
    let mut options = options(&fx, gh);
    options.bus = Some(Arc::clone(&bus));
    let watcher = started(options);
    tick(&fx, &watcher, WATCH_INTERVAL_MS * 3);
    assert_eq!(count(&calls), 0);

    let leave = bus.subscribe_client(|_| {});
    watcher.wake();
    watcher.settle();
    assert_eq!(count(&calls), 1);

    bus.unsubscribe(leave);
    tick(&fx, &watcher, SETTLED_INTERVAL_MS * 3);
    assert_eq!(count(&calls), 1);
    watcher.stop();
}

#[test]
fn asks_every_5_minutes_once_all_checks_settled_sooner_for_a_new_pull_request() {
    let fx = Fx::new();
    task_with_pr(&fx, 1, None);
    let (gh, calls) = graphql(vec![(1, "OPEN"), (2, "OPEN")]);
    let mut options = options(&fx, gh);
    options.bus = Some(bus_with_client());

    // A dashboard listens already: the first check is at once.
    let watcher = started(options);
    assert_eq!(count(&calls), 1);
    tick(&fx, &watcher, SETTLED_INTERVAL_MS - WATCH_INTERVAL_MS);
    assert_eq!(count(&calls), 1);
    tick(&fx, &watcher, WATCH_INTERVAL_MS);
    assert_eq!(count(&calls), 2);

    task_with_pr(&fx, 2, None);
    tick(&fx, &watcher, WATCH_INTERVAL_MS);
    assert_eq!(count(&calls), 3);
    watcher.stop();
}

#[test]
fn slows_down_while_gh_keeps_failing() {
    let fx = Fx::new();
    task_with_pr(&fx, 1, None);
    let (gh, calls) = fails("HTTP 502");
    let mut options = options(&fx, gh);
    options.bus = Some(bus_with_client());

    let watcher = started(options);
    tick(&fx, &watcher, WATCH_INTERVAL_MS * 2);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    tick(&fx, &watcher, WATCH_INTERVAL_MS * 3);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    tick(&fx, &watcher, WATCH_INTERVAL_MS);
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    watcher.stop();
}

#[test]
fn asks_every_minute_again_once_a_settled_pull_request_has_a_check_that_runs() {
    let fx = Fx::new();
    let task = task_with_pr(&fx, 1, None);
    let checks: Arc<Mutex<Vec<Value>>> = Arc::default();
    let rollup = Arc::clone(&checks);
    let (gh, calls) = recording(move |_| {
        let nodes = rollup.lock().unwrap().clone();
        let commit = json!({ "statusCheckRollup": { "contexts": { "nodes": nodes } } });
        let node = json!({
            "state": "OPEN", "isDraft": false, "commits": { "nodes": [{ "commit": commit }] },
        });
        Ok(json!({ "data": { "repository": { "pr1": node } } }).to_string())
    });
    let mut options = options(&fx, gh);
    options.bus = Some(bus_with_client());
    let watcher = started(options);
    assert_eq!(count(&calls), 1);

    // A new commit was pushed: GitHub runs its checks.
    *checks.lock().unwrap() = vec![json!({ "status": "IN_PROGRESS", "conclusion": null })];
    tick(&fx, &watcher, SETTLED_INTERVAL_MS);
    assert_eq!(count(&calls), 2);

    tick(&fx, &watcher, WATCH_INTERVAL_MS);
    assert_eq!(count(&calls), 3);
    assert_eq!(
        fx.tasks.get_task(&task.id).unwrap().pr_status,
        Some(status("open", "pending"))
    );
    tick(&fx, &watcher, WATCH_INTERVAL_MS);
    assert_eq!(count(&calls), 4);
    watcher.stop();
}
