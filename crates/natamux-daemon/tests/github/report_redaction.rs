//! The pull request check log never holds the token of a URL or a gh token.

use natamux_daemon::github::pull_request_watcher::PullRequestWatcher;

use super::Fx;
use super::pull_request_watcher::{fails, logged, options, task_with_pr};

#[test]
fn a_failed_check_is_logged_without_its_token() {
    let fx = Fx::new();
    task_with_pr(&fx, 1, None);
    let (gh, _) = fails("fatal: https://user:FAKE-TOKEN@host.test/x.git ghp_FAKEFAKE1234");
    let mut options = options(&fx, gh);
    let lines = logged(&mut options);
    let watcher = PullRequestWatcher::new(options);

    watcher.check_all();

    let lines = lines.lock().unwrap().clone();
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(!lines[0].contains("FAKE"), "{lines:?}");
    assert!(lines[0].contains("https://host.test/x.git"), "{lines:?}");
}
