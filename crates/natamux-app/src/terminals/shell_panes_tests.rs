//! The grid confirms a listed shell with no cached terminal by a new list read (T-0211).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;
use std::time::Duration;

use gpui::{Entity, TestAppContext, VisualTestContext};
use natamux_proto::terminals::list::TerminalList;
use ui::prelude::*;

use super::ShellPanes;
use crate::app::dashboard::Dashboard;
use crate::app::source::Source;
use crate::app::test_views;
use crate::terminals::terminal_cache::TerminalCache;

fn listed(names: &[&str]) -> TerminalList {
    let names: Vec<String> = names.iter().map(|name| name.to_string()).collect();
    TerminalList {
        terminals: names.clone(),
        agents: names,
        ..TerminalList::default()
    }
}

/// The grid of `task_id` while the dashboard still holds `held`, an older list than the fake
/// daemon's.
fn open_task<'a>(
    task_id: &str,
    held: TerminalList,
    cx: &'a mut TestAppContext,
) -> (Entity<Dashboard>, &'a mut VisualTestContext) {
    test_views::init(cx);
    let dashboard = cx.new(|cx| Dashboard::new(Ok(Arc::new(Source::FakeDaemon)), cx));
    cx.run_until_parked();
    dashboard.update(cx, |dashboard, _| {
        dashboard.data.terminals.insert(task_id.to_owned(), held);
    });
    let shown = dashboard.clone();
    let (panes, cx) = cx.add_window_view(move |_, cx| ShellPanes::new(shown, cx));
    let id = task_id.to_owned();
    panes.update(cx, |panes, cx| panes.set_task(Some(id), cx));
    cx.run_until_parked();
    (dashboard, cx)
}

fn cached(key: &str, cx: &mut VisualTestContext) -> bool {
    cx.update(|_, cx| TerminalCache::cached(key, cx).is_some())
}

/// T-0211: the daemon closed Terminal 8 of T-0210 (a stage that ends its shell) while another
/// task was shown, so the list the app held still had it. Opening the task made a terminal for
/// it, and its first attach started a new shell under the name.
#[gpui::test]
fn a_listed_shell_the_daemon_closed_gets_no_terminal(cx: &mut TestAppContext) {
    // The fake daemon lists only Terminal 1 for T-0141.
    let (dashboard, cx) = open_task("T-0141", listed(&["1", "8"]), cx);

    assert!(
        !cached("T-0141/8", cx),
        "a terminal for the closed shell starts a new one"
    );
    assert!(
        cached("T-0141/1", cx),
        "Terminal 1 did not show after the read"
    );
    let list = dashboard.read_with(cx, |dashboard, _| {
        dashboard.data.terminals["T-0141"].clone()
    });
    assert_eq!(list.terminals, ["1"]);
}

#[gpui::test]
fn a_listed_shell_the_daemon_still_has_gets_its_terminal_after_the_read(cx: &mut TestAppContext) {
    // The fake daemon lists Terminals 1, 2, 10 and 12 for T-0146.
    let (_, cx) = open_task("T-0146", listed(&["1", "2", "10", "12"]), cx);

    for name in ["1", "2", "10", "12"] {
        assert!(
            cached(&format!("T-0146/{name}"), cx),
            "Terminal {name} did not show"
        );
    }
}

/// Shows the grid, or another view in its place (the Agents grid, Settings).
struct Host {
    panes: Entity<ShellPanes>,
    shown: bool,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .when(self.shown, |host| host.child(self.panes.clone()))
    }
}

/// The grid of T-0141 in a host, before it has a task. The fake daemon lists only Terminal 1 for
/// T-0141. The window draws at the end of each update; reads answer only when the executor runs.
fn hosted(cx: &mut TestAppContext) -> (Entity<Dashboard>, Entity<Host>, &mut VisualTestContext) {
    test_views::init(cx);
    let dashboard = cx.new(|cx| Dashboard::new(Ok(Arc::new(Source::FakeDaemon)), cx));
    cx.run_until_parked();
    let shown = dashboard.clone();
    let (host, cx) = cx.add_window_view(move |_, cx| Host {
        panes: cx.new(|cx| ShellPanes::new(shown, cx)),
        shown: true,
    });
    (dashboard, host, cx)
}

fn hold(dashboard: &Entity<Dashboard>, names: &[&str], cx: &mut VisualTestContext) {
    let list = listed(names);
    dashboard.update(cx, |dashboard, _| {
        dashboard.data.terminals.insert("T-0141".into(), list);
    });
}

fn show(host: &Entity<Host>, shown: bool, cx: &mut VisualTestContext) {
    host.update(cx, |host, cx| {
        host.shown = shown;
        host.panes.update(cx, |_, cx| cx.notify());
        cx.notify();
    });
}

fn open_task_in(host: &Entity<Host>, cx: &mut VisualTestContext) {
    host.update(cx, |host, cx| {
        host.panes
            .update(cx, |panes, cx| panes.set_task(Some("T-0141".into()), cx));
    });
}

/// Review of T-0211: the client's own read of a newly shown task can land before the grid's read,
/// so the grid stops waiting. Its read answers later, while another view fills the window. That
/// answer must not end the next wait, or a closed shell of a stale list gets a terminal unread.
#[gpui::test]
fn an_answer_the_grid_no_longer_waits_for_ends_no_later_wait(cx: &mut TestAppContext) {
    let (dashboard, host, cx) = hosted(cx);
    hold(&dashboard, &["1", "8"], cx);
    open_task_in(&host, cx);
    hold(&dashboard, &["1"], cx);
    show(&host, true, cx);
    show(&host, false, cx);
    cx.run_until_parked();

    // The daemon closed Terminal 9 while the list was not read.
    hold(&dashboard, &["1", "9"], cx);
    show(&host, true, cx);
    cx.run_until_parked();

    assert!(
        !cached("T-0141/9", cx),
        "the old answer let Terminal 9 skip its read"
    );
    assert!(cached("T-0141/1", cx), "Terminal 1 did not show");
}

/// Review of T-0211: the grid's read answers while another view fills the window. Minutes later
/// the grid shows again with a stale list, so that answer is too old to end the wait.
#[gpui::test]
fn an_answer_that_waited_long_for_a_draw_ends_no_wait(cx: &mut TestAppContext) {
    let (dashboard, host, cx) = hosted(cx);
    hold(&dashboard, &["1", "8"], cx);
    open_task_in(&host, cx);
    show(&host, false, cx);
    cx.run_until_parked();

    cx.executor().advance_clock(Duration::from_secs(60));
    // The daemon closed Terminal 9 while the list was not read.
    hold(&dashboard, &["1", "9"], cx);
    show(&host, true, cx);
    cx.run_until_parked();

    assert!(
        !cached("T-0141/9", cx),
        "the old answer let Terminal 9 skip its read"
    );
    assert!(cached("T-0141/1", cx), "Terminal 1 did not show");
}
