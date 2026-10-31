//! The tree builds its lines again only when what they show changed, and draws again only for
//! the dashboard parts its rows read (T-0223, P2).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use gpui::{Entity, StyleRefinement, TestAppContext, VisualTestContext};
use natamux_client::DashboardEvent;
use natamux_proto::agents::activity::{TaskActivity, TaskUsage};
use natamux_proto::workspaces::workspace::DiffStat;
use ui::prelude::*;

use super::WorkspaceTree;
use crate::app::dashboard::Dashboard;
use crate::app::source::Source;
use crate::app::test_views;
use crate::sidebar::tree_lines::{LinesChange, changed_lines};
use crate::sidebar::tree_model::Line;

/// The sidebar caches the tree as Root caches the sidebar.
struct Host {
    tree: Entity<WorkspaceTree>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.tree.clone().cached(full()))
    }
}

fn full() -> StyleRefinement {
    StyleRefinement::default().size_full()
}

fn sidebar_tree(
    cx: &mut TestAppContext,
) -> (
    Entity<Dashboard>,
    Entity<WorkspaceTree>,
    &mut VisualTestContext,
) {
    test_views::init(cx);
    let source = Arc::new(Source::FakeDaemon);
    let dashboard = cx.new(|cx| Dashboard::new(Ok(source), Default::default(), cx));
    cx.run_until_parked();
    let shown = dashboard.clone();
    let (host, cx) = cx.add_window_view(move |_, cx| Host {
        tree: cx.new(|cx| WorkspaceTree::new(shown, cx)),
    });
    cx.run_until_parked();
    let tree = host.read_with(cx, |host, _| host.tree.clone());
    (dashboard, tree, cx)
}

fn send(dashboard: &Entity<Dashboard>, event: DashboardEvent, cx: &mut VisualTestContext) {
    dashboard.update(cx, |dashboard, cx| dashboard.take_event(event, cx));
    cx.run_until_parked();
}

/// The builds of the lines and the draws of the tree.
fn counts(tree: &Entity<WorkspaceTree>, cx: &mut VisualTestContext) -> (usize, usize) {
    tree.read_with(cx, |tree, _| (tree.builds, tree.draws))
}

#[gpui::test]
fn usage_builds_and_draws_nothing_but_diff_stats_draw_without_a_build(cx: &mut TestAppContext) {
    let (dashboard, tree, cx) = sidebar_tree(cx);
    let before = counts(&tree, cx);
    assert!(before.0 > 0 && before.1 > 0, "the tree never drew");
    let usage = TaskUsage {
        tokens: 99,
        ..TaskUsage::default()
    };
    send(
        &dashboard,
        DashboardEvent::Usage([("T-0146".to_string(), usage)].into()),
        cx,
    );
    assert_eq!(counts(&tree, cx), before);
    let stat = DiffStat {
        files: 3,
        added: 40,
        removed: 2,
    };
    send(
        &dashboard,
        DashboardEvent::DiffStats([("T-0146".to_string(), stat)].into()),
        cx,
    );
    let (builds, draws) = counts(&tree, cx);
    assert_eq!(builds, before.0);
    assert!(draws > before.1);
    let shown = tree.update_in(cx, |tree, window, cx| tree.snapshot(window, cx).diff_stats);
    assert_eq!(shown.get("T-0146"), Some(&stat));
}

#[gpui::test]
fn the_running_glyph_follows_terminals_and_activity(cx: &mut TestAppContext) {
    let (dashboard, tree, cx) = sidebar_tree(cx);
    let (builds, draws) = counts(&tree, cx);

    // A new terminal: the rows under the task are built again.
    let held = dashboard.read_with(cx, |dashboard, _| dashboard.data.terminals.clone());
    let mut list = held.get("T-0146").cloned().unwrap_or_default();
    list.terminals.push("Terminal 9".into());
    let terminals = DashboardEvent::Terminals {
        task_id: "T-0146".into(),
        terminals: list,
    };
    send(&dashboard, terminals, cx);
    let (built, drawn) = counts(&tree, cx);
    assert!(built > builds && drawn > draws);

    // A new line of an agent: the same lines, drawn again.
    let mut activity = dashboard.read_with(cx, |dashboard, _| dashboard.data.activity.clone());
    activity.insert("T-9999".into(), TaskActivity::new());
    send(&dashboard, DashboardEvent::Activity(activity), cx);
    let (rebuilt, redrawn) = counts(&tree, cx);
    assert_eq!(rebuilt, built);
    assert!(redrawn > drawn);
}

#[gpui::test]
fn a_pin_still_builds_the_tree_again(cx: &mut TestAppContext) {
    let (dashboard, tree, cx) = sidebar_tree(cx);
    let (builds, _) = counts(&tree, cx);
    dashboard.update(cx, |dashboard, cx| dashboard.toggle_pin("T-0146", cx));
    cx.run_until_parked();
    assert!(counts(&tree, cx).0 > builds);
}

#[gpui::test]
fn the_task_ids_switch_shows_the_ids_at_once(cx: &mut TestAppContext) {
    let (dashboard, tree, cx) = sidebar_tree(cx);
    let shown = |tree: &Entity<WorkspaceTree>, cx: &mut VisualTestContext| {
        tree.update_in(cx, |tree, window, cx| tree.snapshot(window, cx).show_ids)
    };
    assert!(!shown(&tree, cx));
    let (_, draws) = counts(&tree, cx);
    dashboard.update(cx, |dashboard, cx| dashboard.toggle_sidebar_ids(cx));
    cx.run_until_parked();
    assert!(counts(&tree, cx).1 > draws);
    assert!(shown(&tree, cx));
}

#[test]
fn only_the_changed_lines_are_measured_again() {
    let header = |group| Line::GroupHeader { group };
    let lines = [header(0), header(1), header(2), header(3)];
    assert_eq!(changed_lines(&lines, &lines), LinesChange::Same);
    let moved = [header(0), header(5), header(6), header(3)];
    assert_eq!(changed_lines(&lines, &moved), LinesChange::Range(1..3));
    assert_eq!(changed_lines(&lines, &lines[..3]), LinesChange::Count);
    assert_eq!(changed_lines(&[], &lines), LinesChange::Count);
}
