//! All tasks sorts and draws again only for the dashboard parts it reads (T-0223, P2).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::sync::Arc;

use gpui::{AppContext as _, Entity, TestAppContext, VisualTestContext};
use natamux_client::DashboardEvent;
use natamux_proto::agents::activity::{TaskActivity, TaskUsage};
use natamux_proto::workspaces::workspace::DiffStat;

use super::TaskView;
use crate::app::dashboard::Dashboard;
use crate::app::source::Source;
use crate::app::test_views;
use crate::app::view_state::Layout;
use crate::tasks::task_sort::SortKey;

fn all_tasks(
    cx: &mut TestAppContext,
) -> (Entity<Dashboard>, Entity<TaskView>, &mut VisualTestContext) {
    test_views::init(cx);
    let source = Arc::new(Source::FakeDaemon);
    let dashboard = cx.new(|cx| Dashboard::new(Ok(source), Default::default(), cx));
    cx.run_until_parked();
    let shown = dashboard.clone();
    let (view, cx) = cx.add_window_view(move |window, cx| TaskView::new(shown, window, cx));
    cx.run_until_parked();
    (dashboard, view, cx)
}

fn send(dashboard: &Entity<Dashboard>, event: DashboardEvent, cx: &mut VisualTestContext) {
    dashboard.update(cx, |dashboard, cx| dashboard.take_event(event, cx));
    cx.run_until_parked();
}

fn refreshes(view: &Entity<TaskView>, cx: &mut VisualTestContext) -> usize {
    view.read_with(cx, |view, _| view.refreshes)
}

fn board_draws(view: &Entity<TaskView>, cx: &mut VisualTestContext) -> usize {
    view.read_with(cx, |view, cx| view.board.read(cx).draws)
}

/// The data held, with one more task in it: a change of the part.
fn activity(dashboard: &Entity<Dashboard>, cx: &mut VisualTestContext) -> DashboardEvent {
    let mut activity = dashboard.read_with(cx, |dashboard, _| dashboard.data.activity.clone());
    let next = activity.len();
    activity.insert(format!("T-{next}"), TaskActivity::new());
    DashboardEvent::Activity(activity)
}

fn usage(costs: &[(&str, f64)]) -> DashboardEvent {
    let usage = costs.iter().map(|(id, cost)| {
        let usage = TaskUsage {
            cost_usd: Some(*cost),
            ..TaskUsage::default()
        };
        (id.to_string(), usage)
    });
    DashboardEvent::Usage(usage.collect())
}

fn diff_stats(sizes: &[(&str, u64)]) -> DashboardEvent {
    let stats: BTreeMap<String, DiffStat> = sizes
        .iter()
        .map(|(id, added)| {
            let stat = DiffStat {
                files: 1,
                added: *added,
                removed: 0,
            };
            (id.to_string(), stat)
        })
        .collect();
    DashboardEvent::DiffStats(stats)
}

/// Whether T-0146 shows before T-0147.
fn first_before_second(view: &Entity<TaskView>, cx: &mut VisualTestContext) -> bool {
    view.read_with(cx, |view, _| {
        let at = |id: &str| view.shown_ids.iter().position(|shown| shown == id);
        at("T-0146") < at("T-0147")
    })
}

#[gpui::test]
fn activity_usage_and_diff_stats_do_not_sort_the_default_order_again(cx: &mut TestAppContext) {
    let (dashboard, view, cx) = all_tasks(cx);
    let before = refreshes(&view, cx);
    let event = activity(&dashboard, cx);
    send(&dashboard, event, cx);
    send(&dashboard, usage(&[("T-0146", 1.0)]), cx);
    send(&dashboard, diff_stats(&[("T-0146", 10)]), cx);
    assert_eq!(refreshes(&view, cx), before);
}

#[gpui::test]
fn an_activity_event_draws_no_board(cx: &mut TestAppContext) {
    let (dashboard, view, cx) = all_tasks(cx);
    dashboard.update(cx, |dashboard, cx| dashboard.set_layout(Layout::Board, cx));
    cx.run_until_parked();
    let (refreshed, drawn) = (refreshes(&view, cx), board_draws(&view, cx));
    assert!(drawn > 0, "the board never drew");

    let event = activity(&dashboard, cx);
    send(&dashboard, event, cx);
    assert_eq!(refreshes(&view, cx), refreshed);
    assert_eq!(board_draws(&view, cx), drawn, "the board drew for activity");

    // The cards show the diff stats.
    send(&dashboard, diff_stats(&[("T-0146", 10)]), cx);
    assert!(board_draws(&view, cx) > drawn);
}

#[gpui::test]
fn a_sort_by_changes_or_by_cost_sorts_again_on_its_part(cx: &mut TestAppContext) {
    let (dashboard, view, cx) = all_tasks(cx);
    dashboard.update(cx, |dashboard, cx| dashboard.sort_by(SortKey::Changes, cx));
    send(&dashboard, diff_stats(&[("T-0146", 10), ("T-0147", 500)]), cx);
    let first = first_before_second(&view, cx);
    send(&dashboard, diff_stats(&[("T-0146", 500), ("T-0147", 10)]), cx);
    assert_ne!(first_before_second(&view, cx), first);

    dashboard.update(cx, |dashboard, cx| dashboard.sort_by(SortKey::Cost, cx));
    send(&dashboard, usage(&[("T-0146", 1.0), ("T-0147", 9.0)]), cx);
    let first = first_before_second(&view, cx);
    send(&dashboard, usage(&[("T-0146", 9.0), ("T-0147", 1.0)]), cx);
    assert_ne!(first_before_second(&view, cx), first);
}

/// One change of the dashboard's own state.
type Step = dyn Fn(&mut Dashboard, &mut gpui::Context<Dashboard>);

#[gpui::test]
fn a_filter_a_sort_or_a_selection_still_refreshes(cx: &mut TestAppContext) {
    let (dashboard, view, cx) = all_tasks(cx);
    let mut before = refreshes(&view, cx);
    let steps: [&Step; 3] = [
        &|dashboard, cx| dashboard.set_filter("T-0146".into(), cx),
        &|dashboard, cx| dashboard.sort_by(SortKey::Id, cx),
        &|dashboard, cx| dashboard.select(Some("T-0146".into()), cx),
    ];
    for step in steps {
        dashboard.update(cx, |dashboard, cx| step(dashboard, cx));
        cx.run_until_parked();
        let after = refreshes(&view, cx);
        assert!(after > before);
        before = after;
    }
    view.read_with(cx, |view, _| {
        assert!(view.shown_ids.iter().any(|id| id == "T-0146"));
        assert!(!view.shown_ids.iter().any(|id| id == "T-0137"));
    });
}
