//! `pull-request-watcher.test.ts`: the group 'PullRequestWatcher finds a pull request opened
//! outside Natamux', and `isCandidate`.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use natamux_daemon::github::pull_request_watcher::{
    PullRequestWatcher, PullRequestWatcherOptions, SETTLED_INTERVAL_MS, WATCH_INTERVAL_MS,
    is_candidate,
};
use natamux_daemon::shared::event_bus::DaemonMessage;
use natamux_git::gh::{GitHubError, RunGh};
use natamux_proto::events::task_event::TaskEvent;
use natamux_proto::tasks::task::{Task, TaskState};
use serde_json::{Map, Value, json};

use super::pull_request_watcher::{
    Found, bus_with_client, count, counting_no_branch, fails, github, github_with, logged,
    on_origin, options, pr_events, started, status, task_on_branch, tick,
};
use super::{Fx, GhCall};

#[test]
fn looks_for_a_pull_request_of_an_active_task_that_has_a_branch_and_none_yet() {
    let fx = Fx::new();
    let task = task_on_branch(&fx, "natamux/t-1", None);

    assert!(is_candidate(&task));
    let pr = Some("https://github.com/o/r/pull/1".to_string());
    assert!(!is_candidate(&Task { pr, ..task.clone() }));
    assert!(!is_candidate(&Task {
        branch: None,
        ..task.clone()
    }));
    assert!(!is_candidate(&Task {
        workspace: None,
        ..task.clone()
    }));
    for state in [TaskState::Cancelled, TaskState::Done] {
        assert!(!is_candidate(&Task {
            state,
            ..task.clone()
        }));
    }
}

const URL: &str = "https://github.com/o/r/pull/5";

fn found(state: &'static str) -> Vec<Found> {
    vec![Found {
        branch: "natamux/t-1",
        url: URL,
        state,
    }]
}

fn finding(fx: &Fx, gh: RunGh) -> PullRequestWatcherOptions {
    let mut options = options(fx, gh);
    options.has_remote_branch = Some(on_origin());
    options
}

#[test]
fn links_the_pull_request_of_the_task_branch_and_reads_its_status() {
    let fx = Fx::new();
    let task = task_on_branch(&fx, "natamux/t-1", None);
    let (gh, calls) = github(found("OPEN"));

    PullRequestWatcher::new(finding(&fx, gh)).check_all();

    let linked = fx.tasks.get_task(&task.id).unwrap();
    assert_eq!(linked.pr.as_deref(), Some(URL));
    assert_eq!(linked.pr_status, Some(status("open", "none")));
    assert_eq!(calls.lock().unwrap()[0].cwd, "/work/natamux/t-1");
    let opened: Vec<Value> = pr_events(&fx, &task.id)
        .into_iter()
        .map(Value::Object)
        .collect();
    assert_eq!(opened, [json!({ "url": URL, "detected": "true" })]);
}

#[test]
fn lists_the_pull_requests_once_per_project_in_the_workspace_of_one_of_its_tasks() {
    let fx = Fx::new();
    let first = task_on_branch(&fx, "natamux/t-1", None);
    let second = task_on_branch(&fx, "natamux/t-2", None);
    task_on_branch(&fx, "natamux/t-3", None);
    let web = task_on_branch(&fx, "natamux/t-4", Some("web"));
    let pr = |branch, url, state| Found { branch, url, state };
    let (gh, calls) = github(vec![
        pr("natamux/t-1", "https://github.com/o/r/pull/1", "OPEN"),
        pr("natamux/t-2", "https://github.com/o/r/pull/2", "MERGED"),
        pr("natamux/t-4", "https://github.com/o/web/pull/4", "OPEN"),
    ]);

    PullRequestWatcher::new(finding(&fx, gh)).check_all();

    let calls = calls.lock().unwrap();
    let lists: Vec<&GhCall> = calls
        .iter()
        .filter(|call| call.args[0] == "pr" && call.args[1] == "list")
        .collect();
    assert_eq!(lists.len(), 2);
    // Any task of the project will do.
    let mut cwds: Vec<&str> = lists.iter().map(|call| call.cwd.as_str()).collect();
    cwds.sort_unstable();
    let default = [
        "/work/natamux/t-1",
        "/work/natamux/t-2",
        "/work/natamux/t-3",
    ];
    assert!(default.contains(&cwds[0]));
    assert_eq!(cwds[1], "/work/natamux/t-4");
    assert!(
        lists
            .iter()
            .all(|call| !call.args.contains(&"--head".to_string()))
    );
    let pr_of = |id: &str| fx.tasks.get_task(id).unwrap().pr;
    assert_eq!(
        pr_of(&first.id).as_deref(),
        Some("https://github.com/o/r/pull/1")
    );
    assert_eq!(
        pr_of(&second.id).as_deref(),
        Some("https://github.com/o/r/pull/2")
    );
    assert_eq!(
        pr_of(&web.id).as_deref(),
        Some("https://github.com/o/web/pull/4")
    );
}

#[test]
fn goes_on_with_the_other_projects_when_gh_fails_for_one() {
    let fx = Fx::new();
    task_on_branch(&fx, "natamux/t-1", None);
    let web = task_on_branch(&fx, "natamux/t-2", Some("web"));
    let (gh, _) = github_with(
        vec![Found {
            branch: "natamux/t-2",
            url: URL,
            state: "OPEN",
        }],
        |call| (call.cwd == "/work/natamux/t-1").then(|| Err(GitHubError::new("HTTP 502"))),
    );
    let mut options = finding(&fx, gh);
    let lines = logged(&mut options);

    PullRequestWatcher::new(options).check_all();

    assert_eq!(fx.tasks.get_task(&web.id).unwrap().pr.as_deref(), Some(URL));
    assert_eq!(
        *lines.lock().unwrap(),
        ["Pull request check: GitHub CLI failed: HTTP 502"]
    );
}

#[test]
fn does_not_link_a_pull_request_twice_when_a_check_linked_it_meanwhile() {
    let fx = Fx::new();
    let task = task_on_branch(&fx, "natamux/t-1", None);
    let tasks = fx.tasks.clone();
    let id = task.id.clone();
    // The first list call links it by hand, as a check on another thread would.
    let (gh, _) = github_with(found("OPEN"), move |call| {
        if call.args[1] == "list" && tasks.get_task(&id).unwrap().pr.is_none() {
            tasks
                .record_git(&id, "pr.opened", json!({ "url": URL }))
                .unwrap();
        }
        None
    });

    PullRequestWatcher::new(finding(&fx, gh)).check_all();

    assert_eq!(pr_events(&fx, &task.id).len(), 1);
}

#[test]
fn asks_github_nothing_while_origin_does_not_have_the_branch() {
    let fx = Fx::new();
    let task = task_on_branch(&fx, "natamux/t-1", None);
    let (gh, calls) = github(found("OPEN"));
    let mut options = options(&fx, gh);
    options.has_remote_branch = Some(counting_no_branch(&Arc::default()));

    PullRequestWatcher::new(options).check_all();

    assert_eq!(count(&calls), 0);
    assert_eq!(fx.tasks.get_task(&task.id).unwrap().pr, None);
}

#[test]
fn does_not_link_a_closed_pull_request() {
    let fx = Fx::new();
    let task = task_on_branch(&fx, "natamux/t-1", None);
    let (gh, _) = github(found("CLOSED"));

    PullRequestWatcher::new(finding(&fx, gh)).check_all();

    assert_eq!(fx.tasks.get_task(&task.id).unwrap().pr, None);
}

#[test]
fn looks_when_asked_to_check_a_task_without_a_pull_request() {
    let fx = Fx::new();
    let task = task_on_branch(&fx, "natamux/t-1", None);
    let (gh, _) = github(found("MERGED"));

    let checked = PullRequestWatcher::new(finding(&fx, gh))
        .check(&task.id)
        .unwrap();

    assert_eq!(checked.pr.as_deref(), Some(URL));
    assert_eq!(
        checked.pr_status.map(|s| s.state).as_deref(),
        Some("merged")
    );
}

#[test]
fn looks_when_the_agent_of_the_task_stops() {
    let fx = Fx::new();
    let task = task_on_branch(&fx, "natamux/t-1", None);
    let (gh, _) = github(found("OPEN"));
    let mut options = finding(&fx, gh);
    options.bus = Some(Arc::clone(&fx.bus));
    let watcher = started(options);

    let event = TaskEvent {
        task_id: task.id.clone(),
        source: "system".into(),
        kind: "agent.stopped".into(),
        at: "2026-10-24T09:00:00.000Z".into(),
        payload: Map::new(),
    };
    fx.bus.publish(DaemonMessage::TaskUpdated {
        task: task.clone(),
        event,
        accepted: true,
        moved: None,
    });
    watcher.settle();

    let looked = fx.tasks.get_task(&task.id).unwrap();
    assert_eq!(looked.pr_status.map(|s| s.state).as_deref(), Some("open"));
    assert_eq!(looked.pr.as_deref(), Some(URL));
    watcher.stop();
}

#[test]
fn asks_every_minute_while_origin_has_the_branch_of_a_task_with_no_pull_request() {
    let fx = Fx::new();
    task_on_branch(&fx, "natamux/t-1", None);
    let (gh, calls) = github(vec![]);
    let mut options = finding(&fx, gh);
    options.bus = Some(bus_with_client());

    let watcher = started(options);
    assert_eq!(count(&calls), 1);
    tick(&fx, &watcher, WATCH_INTERVAL_MS);
    assert_eq!(count(&calls), 2);
    tick(&fx, &watcher, WATCH_INTERVAL_MS);
    assert_eq!(count(&calls), 3);
    watcher.stop();
}

#[test]
fn asks_every_5_minutes_while_origin_does_not_have_the_branch() {
    let fx = Fx::new();
    task_on_branch(&fx, "natamux/t-1", None);
    let (gh, _) = github(vec![]);
    let looks = Arc::new(AtomicUsize::new(0));
    let mut options = options(&fx, gh);
    options.has_remote_branch = Some(counting_no_branch(&looks));
    options.bus = Some(bus_with_client());

    let watcher = started(options);
    assert_eq!(looks.load(Ordering::SeqCst), 1);
    tick(&fx, &watcher, SETTLED_INTERVAL_MS - WATCH_INTERVAL_MS);
    assert_eq!(looks.load(Ordering::SeqCst), 1);
    tick(&fx, &watcher, WATCH_INTERVAL_MS);
    assert_eq!(looks.load(Ordering::SeqCst), 2);
    watcher.stop();
}

#[test]
fn still_slows_down_to_5_minutes_while_gh_keeps_failing_for_such_a_task() {
    let fx = Fx::new();
    task_on_branch(&fx, "natamux/t-1", None);
    let (gh, calls) = fails("HTTP 502");
    let mut options = finding(&fx, gh);
    options.bus = Some(bus_with_client());

    let watcher = started(options);
    tick(&fx, &watcher, WATCH_INTERVAL_MS * 2);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    tick(&fx, &watcher, WATCH_INTERVAL_MS * 3);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    tick(&fx, &watcher, WATCH_INTERVAL_MS);
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    // From here on, never longer than 5 minutes.
    tick(&fx, &watcher, SETTLED_INTERVAL_MS);
    assert_eq!(calls.load(Ordering::SeqCst), 4);
    tick(&fx, &watcher, SETTLED_INTERVAL_MS);
    assert_eq!(calls.load(Ordering::SeqCst), 5);
    watcher.stop();
}
