//! The tree builds its lines again only when what they show changed, and draws again only for
//! the dashboard parts its rows read (T-0223, P2).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use gpui::{Entity, Modifiers, StyleRefinement, TestAppContext, VisualTestContext};
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

#[gpui::test]
fn the_card_head_centers_the_glyph_on_the_title(cx: &mut TestAppContext) {
    // Top-aligned, the 14 px glyph sat ~3 px above the 13 px title (T-0251).
    let (dashboard, _, cx) = sidebar_tree(cx);
    let centres = |cx: &mut VisualTestContext| {
        let glyph = cx
            .debug_bounds("glyph-T-0141")
            .expect("the card glyph was not drawn");
        let title = cx
            .debug_bounds("title-T-0141")
            .expect("the card title was not drawn");
        (glyph.center().y, title.center().y)
    };
    let (glyph, title) = centres(cx);
    assert!(
        (glyph - title).abs() <= px(1.0),
        "closed: the glyph at {glyph:?} is off the title at {title:?}"
    );

    // Open, the glyph must stay on the title's first line.
    dashboard.update(cx, |dashboard, cx| {
        dashboard.set_fold("task:T-0141", true, cx)
    });
    cx.run_until_parked();
    let (glyph, title) = centres(cx);
    assert!(
        (glyph - title).abs() <= px(1.5),
        "open: the glyph at {glyph:?} is off the title at {title:?}"
    );
}

#[gpui::test]
fn the_open_card_lines_share_one_left_edge(cx: &mut TestAppContext) {
    // The chips and the branch started under the glyph, the agent icon 6 px right of the title
    // (T-0253).
    let (dashboard, _, cx) = sidebar_tree(cx);
    dashboard.update(cx, |dashboard, cx| dashboard.set_fold("task:T-0141", true, cx));
    cx.run_until_parked();
    let bounds = |name: &str, cx: &mut VisualTestContext| {
        cx.debug_bounds(name)
            .unwrap_or_else(|| panic!("{name} was not drawn"))
    };
    let edge = bounds("title-T-0141", cx).left();
    let row = "agent:T-0141:shell:1";
    let content = format!("{row}-content");
    for name in [
        "chips-T-0141",
        "branch-T-0141",
        "waiting-T-0141",
        content.as_str(),
    ] {
        let left = bounds(name, cx).left();
        assert!(
            (left - edge).abs() <= px(1.0),
            "{name} starts at {}, the title at {}",
            left.0,
            edge.0
        );
    }
    let glyph = bounds("glyph-T-0141", cx).center().x;
    let dot = bounds(&format!("{row}-glyph"), cx).center().x;
    assert!(
        (dot - glyph).abs() <= px(1.0),
        "the terminal dot at {} is off the task glyph at {}",
        dot.0,
        glyph.0
    );
}

#[gpui::test]
fn the_card_head_ends_with_the_age_and_no_buttons(cx: &mut TestAppContext) {
    // The hover "..." and fold buttons kept their room even when hidden (T-0254).
    let (dashboard, _, cx) = sidebar_tree(cx);
    let gap = |cx: &mut VisualTestContext| {
        let card = cx
            .debug_bounds("card-T-0141")
            .expect("the card was not drawn");
        let right = cx
            .debug_bounds("right-T-0141")
            .expect("the age label was not drawn");
        card.right() - right.right()
    };
    // Only the right padding (8 or 9 px) and the 1 px border; one button alone is 16 px or more.
    let closed = gap(cx);
    assert!(closed <= px(14.0), "closed: {} px after the age", closed.0);
    dashboard.update(cx, |dashboard, cx| dashboard.set_fold("task:T-0141", true, cx));
    cx.run_until_parked();
    let open = gap(cx);
    assert!(open <= px(14.0), "open: {} px after the age", open.0);

    // A hover once hid the age for the buttons; with no buttons it must stay painted.
    // T-0146 is running, so it shows its age, not a reason.
    assert!(cx.debug_bounds("age-T-0146").is_some(), "no age before hover");
    let title = cx
        .debug_bounds("title-T-0146")
        .expect("the T-0146 title was not drawn");
    cx.simulate_mouse_move(title.center(), None, Modifiers::none());
    cx.run_until_parked();
    assert!(cx.debug_bounds("age-T-0146").is_some(), "hover hid the age");
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
