//! A switch back to a task seen lately draws what the view kept, before any read answers
//! (T-0223, P5).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use gpui::{AppContext as _, Entity, TestAppContext, VisualTestContext};

use super::WorkspaceView;
use crate::app::dashboard::{Dashboard, DashboardChange};
use crate::app::source::Source;
use crate::app::test_views;

// Seed tasks with a workspace; T-0137 has two changed files and a git graph.
const KEPT: &str = "T-0137";
const OTHER: &str = "T-0146";

fn open_view(cx: &mut TestAppContext) -> (Entity<WorkspaceView>, &mut VisualTestContext) {
    test_views::init(cx);
    let source = Arc::new(Source::FakeDaemon);
    let dashboard = cx.new(|cx| Dashboard::new(Ok(source), Default::default(), cx));
    cx.run_until_parked();
    let (view, cx) = cx.add_window_view(move |_, cx| WorkspaceView::new(dashboard, cx));
    cx.run_until_parked();
    (view, cx)
}

fn show(view: &Entity<WorkspaceView>, id: &str, cx: &mut VisualTestContext) {
    view.update(cx, |view, cx| view.show(id, cx));
}

#[gpui::test]
fn a_switch_back_draws_the_kept_git_data_before_any_read_answers(cx: &mut TestAppContext) {
    let (view, cx) = open_view(cx);
    show(&view, KEPT, cx);
    cx.run_until_parked();
    show(&view, OTHER, cx);
    cx.run_until_parked();

    // One update: no read can answer between the switch and the check.
    let (changes, graph) = view.update(cx, |view, cx| {
        view.show(KEPT, cx);
        let shown = view.shown.as_ref().expect("shown");
        (shown.changes.as_ref().map(Vec::len), shown.graph.is_some())
    });
    assert_eq!(changes, Some(2));
    assert!(graph, "the git banner has no graph in the first frame");
}

#[gpui::test]
fn data_changed_while_away_shows_after_one_read(cx: &mut TestAppContext) {
    let (view, cx) = open_view(cx);
    show(&view, KEPT, cx);
    cx.run_until_parked();
    // Stands for an older read: the daemon has two files.
    view.update(cx, |view, _| {
        view.shown.as_mut().expect("shown").changes = Some(Vec::new());
    });
    show(&view, OTHER, cx);
    cx.run_until_parked();
    view.update(cx, |view, cx| {
        view.on_change(&DashboardChange::WorkspaceChanged(KEPT.into()), cx);
        assert!(view.cache.is_stale(KEPT));
    });

    show(&view, KEPT, cx);
    cx.run_until_parked();

    view.read_with(cx, |view, _| {
        assert_eq!(view.current_changes().map(<[_]>::len), Some(2));
        assert!(!view.cache.is_stale(KEPT));
    });
}

#[gpui::test]
fn the_eleventh_task_leaves_the_kept_ones(cx: &mut TestAppContext) {
    let (view, cx) = open_view(cx);
    let ids: Vec<String> = view.read_with(cx, |view, cx| {
        let tasks = &view.dashboard.read(cx).data.tasks;
        tasks.iter().map(|task| task.id.clone()).collect()
    });
    assert!(ids.len() >= 12, "the seed has too few tasks: {}", ids.len());
    for id in &ids[..12] {
        show(&view, id, cx);
    }
    // A stale mark shows which tasks are still kept.
    let kept: Vec<bool> = view.update(cx, |view, _| {
        ids[..12]
            .iter()
            .map(|id| {
                view.cache.mark_stale(id);
                view.cache.is_stale(id)
            })
            .collect()
    });
    // 12 shown in turn: the last is shown, the 10 before it kept, the first one gone.
    assert!(!kept[0]);
    assert!(kept[1..11].iter().all(|kept| *kept));
    assert!(!kept[11]);
}
