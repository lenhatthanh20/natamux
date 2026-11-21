//! A test file is its own process, so the switch stays on for the rest of it. Offline, the pull
//! request watcher starts no timer and asks GitHub nothing; the first part shows that it does ask
//! when the switch is off.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use natamux_daemon::github::pull_request_watcher::{
    CwdOf, Log, PullRequestWatcher, PullRequestWatcherOptions, WATCH_INTERVAL_MS,
};
use natamux_daemon::shared::event_bus::EventBus;
use natamux_daemon::shared::scheduler::Scheduler;
use natamux_daemon::tasks::service::{TaskService, TaskServiceOptions};
use natamux_git::gh::{GitHubError, RunGh};
use natamux_store::task_index::TaskIndex;
use serde_json::json;

fn watcher(
    label: &str,
    calls: &Arc<AtomicUsize>,
    scheduler: &Arc<Scheduler>,
) -> PullRequestWatcher {
    let dir = std::env::temp_dir().join(format!(
        "natamux-offline-{label}-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let tasks = TaskService::new(TaskServiceOptions {
        data_dir: dir,
        index: TaskIndex::open(Path::new(":memory:")).unwrap(),
        bus: EventBus::new(),
        now: None,
        settings_of: None,
        clock: None,
    });
    let task = tasks
        .create_task(Some(&json!({ "title": "Task 1" })), &[], true)
        .unwrap();
    let url = "https://github.com/o/r/pull/1";
    tasks
        .record_git(&task.id, "pr.opened", json!({ "url": url }))
        .unwrap();
    let seen = Arc::clone(calls);
    let gh: RunGh = Arc::new(move |_: &Path, _: &[String]| {
        seen.fetch_add(1, Ordering::SeqCst);
        Err(GitHubError::new("stub"))
    });
    let cwd_of: CwdOf = Arc::new(|_| "/repo".to_string());
    let log: Log = Arc::new(|_| {});
    let mut options = PullRequestWatcherOptions::new(tasks, cwd_of, gh, Arc::clone(scheduler));
    options.log = Some(log);
    PullRequestWatcher::new(options)
}

#[test]
fn the_watcher_asks_github_only_when_online() {
    let calls = Arc::new(AtomicUsize::new(0));
    let scheduler = Scheduler::manual();
    let online = watcher("on", &calls, &scheduler);
    online.start();
    online.settle();
    assert!(calls.load(Ordering::SeqCst) > 0);
    online.stop();

    natamux_core::offline::force(true);
    let calls = Arc::new(AtomicUsize::new(0));
    let scheduler = Scheduler::manual();
    let offline = watcher("off", &calls, &scheduler);
    offline.start();
    scheduler.advance(WATCH_INTERVAL_MS * 3);
    offline.settle();
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}
