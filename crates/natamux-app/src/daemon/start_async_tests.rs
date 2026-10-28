#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};

use futures::executor::block_on;
use gpui::{AppContext as _, TestAppContext};

use super::process::Slot;
use super::start_async::start_on_thread;
use crate::app::dashboard::Dashboard;
use crate::app::dashboard_data::Link;
use crate::app::sign_in::StartProblem;
use crate::app::source::Source;
use crate::app::test_views;

/// Stands for the daemon: dropping it is what stops the real one.
struct Fake(Arc<AtomicBool>);

impl Drop for Fake {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

#[test]
fn a_closed_slot_gives_a_late_value_back() {
    let mut slot = Slot::default();
    assert_eq!(slot.put(1), None);
    assert_eq!(slot.close(), Some(1));
    assert_eq!(slot.put(2), Some(2));
    assert_eq!(slot.close(), None);
}

#[test]
fn a_started_daemon_goes_into_the_slot_and_answers() {
    let stopped = Arc::new(AtomicBool::new(false));
    let slot = Arc::new(Mutex::new(Slot::default()));
    let fake = Fake(Arc::clone(&stopped));
    let answer = start_on_thread(move || Ok(fake), Arc::clone(&slot), |_| 7).expect("thread");
    assert_eq!(block_on(answer).expect("answer"), Ok(7));
    assert!(!stopped.load(Ordering::SeqCst));
    drop(slot.lock().expect("slot").close());
    assert!(stopped.load(Ordering::SeqCst));
}

#[test]
fn a_start_error_answers_with_its_text() {
    let slot = Arc::new(Mutex::new(Slot::<Fake>::default()));
    let answer = start_on_thread(
        || Err(io::Error::other("The daemon stopped while it started")),
        slot,
        |_| (),
    )
    .expect("thread");
    assert_eq!(
        block_on(answer).expect("answer"),
        Err("The daemon stopped while it started".to_owned())
    );
}

#[test]
fn quitting_before_the_start_ends_stops_the_late_daemon() {
    let stopped = Arc::new(AtomicBool::new(false));
    let slot = Arc::new(Mutex::new(Slot::default()));
    let (release, gate) = mpsc::channel::<()>();
    let fake = Fake(Arc::clone(&stopped));
    let start = move || {
        gate.recv().expect("released");
        Ok::<_, io::Error>(fake)
    };
    let answer = start_on_thread(start, Arc::clone(&slot), |_| ()).expect("thread");
    // The app quits while the daemon still starts.
    assert!(slot.lock().expect("slot").close().is_none());
    release.send(()).expect("release");
    assert!(
        block_on(answer).is_err(),
        "a late daemon must not be handed over"
    );
    assert!(stopped.load(Ordering::SeqCst));
    assert!(slot.lock().expect("slot").close().is_none());
}

#[cfg(unix)]
#[test]
fn the_watcher_tells_once_the_daemon_exits() {
    use super::start_async::watch_child;
    use std::time::Duration;

    let child = std::process::Command::new("sh")
        .args(["-c", "exit 0"])
        .spawn()
        .expect("child");
    let child = Arc::new(Mutex::new(child));
    let exited = watch_child(child, Duration::from_millis(10), || false).expect("thread");
    assert!(block_on(exited).is_ok());
}

#[cfg(unix)]
#[test]
fn the_watcher_stops_without_a_word_when_the_app_quits() {
    use super::start_async::watch_child;
    use std::time::Duration;

    let child = std::process::Command::new("sh")
        .args(["-c", "cat >/dev/null"])
        .stdin(std::process::Stdio::piped())
        .spawn()
        .expect("child");
    let child = Arc::new(Mutex::new(child));
    let exited =
        watch_child(Arc::clone(&child), Duration::from_millis(10), || true).expect("thread");
    assert!(block_on(exited).is_err());
    let mut child = child.lock().expect("child");
    child.kill().expect("kill");
    child.wait().expect("reap");
}

#[test]
fn a_problem_without_a_source_picks_the_first_link() {
    assert_eq!(StartProblem::Starting.link(), Link::Loading);
    assert_eq!(
        StartProblem::Down("No app data folder".into()).link(),
        Link::Down {
            reason: "No app data folder".into(),
            retrying: false,
        }
    );
}

#[gpui::test]
fn the_dashboard_shows_skeletons_until_its_daemon_answers(cx: &mut TestAppContext) {
    test_views::init(cx);
    let starting = Err(StartProblem::Starting);
    let dashboard = cx.new(|cx| Dashboard::new(starting, Default::default(), cx));
    cx.run_until_parked();
    dashboard.read_with(cx, |dashboard, _| {
        assert!(dashboard.source().is_none());
        assert_eq!(dashboard.data.link, Link::Loading);
    });
    let source = Arc::new(Source::FakeDaemon);
    dashboard.update(cx, |dashboard, cx| dashboard.reconnect(source, cx));
    cx.run_until_parked();
    dashboard.read_with(cx, |dashboard, _| {
        assert!(dashboard.source().is_some());
        assert_eq!(dashboard.data.link, Link::Live);
    });
}

#[gpui::test]
fn a_late_start_error_shows_why(cx: &mut TestAppContext) {
    test_views::init(cx);
    let starting = Err(StartProblem::Starting);
    let dashboard = cx.new(|cx| Dashboard::new(starting, Default::default(), cx));
    let reason = "The daemon did not answer within 30 seconds";
    dashboard.update(cx, |dashboard, cx| {
        dashboard.start_failed(reason.to_owned(), cx)
    });
    dashboard.read_with(cx, |dashboard, _| {
        assert!(dashboard.source().is_none());
        assert_eq!(
            dashboard.data.link,
            Link::Down {
                reason: reason.to_owned(),
                retrying: false,
            }
        );
    });
}
