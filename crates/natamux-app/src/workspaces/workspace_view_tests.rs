//! A switch back to a task seen lately draws what the view kept, before any read answers
//! (T-0223, P5). The lines and terminals of the other tasks do not draw the view, and the Docs
//! view it caches follows the dashboard by itself.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use gpui::{AppContext as _, Entity, TestAppContext, VisualTestContext};
use natamux_proto::agents::activity::AgentActivity;
use natamux_proto::terminals::list::TerminalList;

use super::WorkspaceView;
use crate::app::dashboard::{Dashboard, DashboardChange};
use crate::app::dashboard_data::Part;
use crate::app::source::Source;
use crate::app::test_views;
use crate::workspaces::docs_tab::DocsTab;

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

/// A new line of Terminal 1 of `id`, as a hook of its agent gives it.
fn agent_line(dashboard: &Entity<Dashboard>, id: &str, text: &str, cx: &mut VisualTestContext) {
    dashboard.update(cx, |dashboard, cx| {
        let line = AgentActivity {
            text: text.into(),
            at: String::new(),
        };
        let lines = dashboard.data.activity.entry(id.to_owned()).or_default();
        lines.insert("1".into(), line);
        dashboard.data.gens.bump(Part::Activity);
        cx.notify();
    });
}

/// How often `view` tells that it changed from now on. A cached view draws only then, and the
/// details panel draws each time the workspace view tells.
fn notifies<T: 'static>(view: &Entity<T>, cx: &mut VisualTestContext) -> Rc<Cell<usize>> {
    let count = Rc::new(Cell::new(0));
    let seen = Rc::clone(&count);
    cx.update(|_, cx| {
        cx.observe(view, move |_, _| seen.set(seen.get() + 1))
            .detach();
    });
    count
}

#[gpui::test]
fn only_the_lines_and_terminals_of_the_shown_task_tell_the_view(cx: &mut TestAppContext) {
    let (view, cx) = open_view(cx);
    show(&view, KEPT, cx);
    cx.run_until_parked();
    let dashboard = view.read_with(cx, |view, _| view.dashboard.clone());
    // The first change after the switch tells the view that it shows another task.
    agent_line(&dashboard, OTHER, "Read a.rs", cx);
    cx.run_until_parked();
    let told = notifies(&view, cx);

    agent_line(&dashboard, OTHER, "Edit a.rs", cx);
    dashboard.update(cx, |dashboard, cx| {
        let list = TerminalList {
            terminals: vec!["1".into(), "9".into()],
            ..TerminalList::default()
        };
        dashboard.data.terminals.insert(OTHER.into(), list);
        dashboard.data.gens.bump(Part::Terminals);
        cx.notify();
    });
    cx.run_until_parked();
    assert_eq!(
        told.get(),
        0,
        "another task's agent drew the open workspace"
    );

    agent_line(&dashboard, KEPT, "Edit b.rs", cx);
    cx.run_until_parked();
    assert!(told.get() > 0, "the line of the shown task did not draw it");
}

/// The workspace caches the Docs view, so what the view draws from the dashboard must tell it.
#[gpui::test]
fn the_cached_docs_view_follows_the_source_switch_and_no_agent_line(cx: &mut TestAppContext) {
    test_views::init(cx);
    let source = Arc::new(Source::FakeDaemon);
    let dashboard = cx.new(|cx| Dashboard::new(Ok(source), Default::default(), cx));
    cx.run_until_parked();
    let shown = dashboard.clone();
    let (docs, cx) =
        cx.add_window_view(move |window, cx| DocsTab::new(shown, KEPT.into(), window, cx));
    cx.run_until_parked();
    // The first change a new observer sees counts as one of its parts.
    agent_line(&dashboard, OTHER, "Read a.rs", cx);
    cx.run_until_parked();
    let told = notifies(&docs, cx);

    agent_line(&dashboard, OTHER, "Edit a.rs", cx);
    cx.run_until_parked();
    assert_eq!(told.get(), 0, "an agent's line drew the Docs view");

    dashboard.update(cx, |dashboard, cx| dashboard.set_docs_source(true, cx));
    cx.run_until_parked();
    assert!(
        told.get() > 0,
        "the Source switch did not draw the Docs view"
    );
}
