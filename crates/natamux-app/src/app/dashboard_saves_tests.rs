//! `view.json` is written soon after a change of the user, later for a seen mark of the feed,
//! at the end of a panel drag, and at quit.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use gpui::{AppContext as _, Entity, TestAppContext};

use super::{LATER, Pending, SOON, Step, Wait, write};
use crate::app::dashboard::{Dashboard, SavedView};
use crate::app::source::Source;
use crate::app::test_views;
use crate::app::view_state::{self, ViewState};

fn temp_folder(name: &str) -> PathBuf {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("natamux-saves-{name}-{unique}"))
}

/// The view state in the folder's file; None: never written.
fn saved(folder: &Path) -> Option<ViewState> {
    let written = folder.join(view_state::FILE_NAME).exists();
    written.then(|| view_state::load(folder))
}

/// A dashboard on the fake daemon that saves to `folder`, after its first task list.
fn dashboard(folder: &Path, cx: &mut TestAppContext) -> Entity<Dashboard> {
    test_views::init(cx);
    let saved = SavedView {
        view: ViewState::default(),
        folder: Some(folder.to_owned()),
    };
    let source = Arc::new(Source::FakeDaemon);
    let dashboard = cx.new(|cx| Dashboard::new(Ok(source), saved, cx));
    cx.run_until_parked();
    dashboard
}

fn wait(time: Duration, cx: &mut TestAppContext) {
    cx.executor().advance_clock(time);
    cx.run_until_parked();
}

#[test]
fn a_seen_mark_waits_longer_than_a_change_of_the_user() {
    let start = Instant::now();
    let mut pending = Pending::default();
    assert_eq!(pending.step(start, false), Step::Idle);
    pending.add(Wait::Later, start);
    assert_eq!(pending.step(start + SOON, false), Step::Sleep(LATER - SOON));
    assert_eq!(pending.step(start + LATER, false), Step::Write);
    assert_eq!(pending.step(start + LATER, false), Step::Idle);
    // A change of the user does not wait for the mark, and the mark goes with it.
    pending.add(Wait::Later, start);
    pending.add(Wait::Soon, start);
    assert_eq!(pending.step(start + SOON, false), Step::Write);
    assert_eq!(pending.step(start + LATER, false), Step::Idle);
}

#[test]
fn a_width_is_written_when_its_drag_ended() {
    let mut now = Instant::now();
    let mut pending = Pending::default();
    // Two seconds of moves: each one waits anew.
    for _ in 0..120 {
        pending.add(Wait::Settled, now);
        assert!(matches!(pending.step(now, true), Step::Sleep(_)));
        now += Duration::from_millis(16);
    }
    // The pointer rests with the button down: the drag has not ended.
    now += SOON;
    assert_eq!(pending.step(now, true), Step::Sleep(SOON));
    now += SOON;
    assert_eq!(pending.step(now, false), Step::Write);
    assert_eq!(pending.step(now, false), Step::Idle);
}

#[test]
fn an_older_write_never_lands_over_a_newer_one() {
    let folder = temp_folder("order");
    let newest = Mutex::new(0);
    let view = ViewState {
        sidebar_ids: true,
        ..ViewState::default()
    };
    write(&newest, 2, &folder, &view).unwrap();
    write(&newest, 1, &folder, &ViewState::default()).unwrap();
    let ids = saved(&folder).map(|saved| saved.sidebar_ids);
    assert_eq!(ids, Some(true));
}

#[gpui::test]
fn a_seen_mark_of_the_feed_is_written_later(cx: &mut TestAppContext) {
    let folder = temp_folder("seen");
    let dashboard = dashboard(&folder, cx);
    let marks = dashboard.read_with(cx, |dashboard, _| dashboard.view().seen.len());
    assert!(marks > 0, "the first task list marked no task seen");
    wait(SOON * 2, cx);
    assert_eq!(
        saved(&folder),
        None,
        "written as soon as a change of the user"
    );
    wait(LATER, cx);
    let written = saved(&folder).map(|saved| saved.seen.is_empty());
    assert_eq!(written, Some(false));
}

#[gpui::test]
fn a_panel_drag_writes_once_it_stopped(cx: &mut TestAppContext) {
    let folder = temp_folder("drag");
    let dashboard = dashboard(&folder, cx);
    for step in 0..10_u8 {
        let width = 300.0 + f32::from(step);
        dashboard.update(cx, |dashboard, cx| dashboard.set_sidebar_width(width, cx));
        wait(Duration::from_millis(100), cx);
        assert_eq!(saved(&folder), None, "written during the drag");
    }
    wait(SOON, cx);
    let width = dashboard.read_with(cx, |dashboard, _| dashboard.view().sidebar_width);
    let written = saved(&folder).map(|saved| saved.sidebar_width);
    assert_eq!(written, Some(width));
}

#[gpui::test]
fn quit_writes_what_still_waits(cx: &mut TestAppContext) {
    let folder = temp_folder("quit");
    let dashboard = dashboard(&folder, cx);
    dashboard.update(cx, |dashboard, cx| dashboard.set_details_width(400.0, cx));
    assert_eq!(saved(&folder), None);
    let width = dashboard.update(cx, |dashboard, _| {
        dashboard.flush_view();
        dashboard.view().details_width
    });
    let written = saved(&folder).map(|saved| saved.details_width);
    assert_eq!(written, Some(width));
    // Nothing waits any more: the loop that slept writes nothing after it.
    dashboard.update(cx, |dashboard, cx| dashboard.set_details_width(410.0, cx));
    dashboard.update(cx, |dashboard, _| dashboard.flush_view());
    wait(LATER * 2, cx);
    let written = saved(&folder).map(|saved| saved.details_width);
    assert_eq!(written, Some(410.0));
}
