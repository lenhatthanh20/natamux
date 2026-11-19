//! Output in one terminal draws that terminal's view: the other terminal, the sidebar, the task
//! list and the details are reused, and no Natamux view is told (AC2, test-spec 1.2). The views
//! between Root and the terminals are not cached, so the cached views inside them are reused.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use gpui::{Entity, TestAppContext, VisualTestContext};
use ui::prelude::*;

use super::terminal_screen;
use crate::app::source::Source;
use crate::app::test_views::{self, Counted, cached_style};
use crate::terminals::remote_terminal::{RemoteTerminal, Target};
use crate::terminals::term::element::layouts;

/// Two panes side by side, as `ShellPanes` shows them, and the rest of the window.
struct Host {
    terminals: Vec<Entity<RemoteTerminal>>,
    rest: Entity<Counted>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let panes = self.terminals.iter().map(|terminal| {
            div()
                .w_1_2()
                .h_full()
                .child(terminal_screen(terminal, false, cx))
        });
        h_flex()
            .size_full()
            .child(self.rest.clone().cached(cached_style()))
            .children(panes)
    }
}

/// Stands for Root, which draws the workspace and its panes as they are, not cached. `Host` is
/// then the workspace: a view on the way to the terminals, with a cached view beside them.
struct Shell {
    host: Entity<Host>,
}

impl Render for Shell {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.host.clone())
    }
}

/// Terminals 1 and 2 of a task of the fake daemon.
fn two_terminals(window: &mut Window, cx: &mut App) -> Vec<Entity<RemoteTerminal>> {
    let source = Arc::new(Source::FakeDaemon);
    ["1", "2"]
        .map(|name| {
            let target = Target {
                task_id: "T-0146".into(),
                terminal: name.into(),
            };
            let source = Arc::clone(&source);
            cx.new(|cx| RemoteTerminal::new(source, target, window, cx))
        })
        .to_vec()
}

/// Runs what is due and draws, lets the timers of the terminal's pace pass, and draws again.
fn settle(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(300));
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
}

/// How often `entity` tells that it changed from now on. A cached view draws only then.
fn notifies<T: 'static>(entity: &Entity<T>, cx: &mut VisualTestContext) -> Rc<Cell<usize>> {
    let count = Rc::new(Cell::new(0));
    let seen = Rc::clone(&count);
    cx.update(|_, cx| {
        cx.observe(entity, move |_, _| seen.set(seen.get() + 1))
            .detach();
    });
    count
}

#[gpui::test]
fn output_draws_only_the_view_of_its_terminal(cx: &mut TestAppContext) {
    test_views::init(cx);
    let (host, cx) = cx.add_window_view(|window, cx| Host {
        terminals: two_terminals(window, cx),
        rest: cx.new(|_| Counted { draws: 0 }),
    });
    // The first layout connects both terminals and builds their views.
    cx.run_until_parked();
    let (terminals, rest) =
        host.read_with(cx, |host, _| (host.terminals.clone(), host.rest.clone()));
    let views: Vec<_> = terminals
        .iter()
        .map(|terminal| terminal.read_with(cx, |terminal, _| terminal.view.clone()))
        .collect();
    let view_notifies: Vec<_> = views.iter().map(|view| notifies(view, cx)).collect();
    let terminal_notifies: Vec<_> = terminals
        .iter()
        .map(|terminal| notifies(terminal, cx))
        .collect();
    let rest_draws = rest.read_with(cx, |rest, _| rest.draws);

    let zed = terminals[0].read_with(cx, |terminal, _| terminal.terminal().clone());
    zed.update(cx, |terminal, cx| {
        terminal.write_output(b"built in 2.1 s\n", cx)
    });
    cx.run_until_parked();

    assert!(
        view_notifies[0].get() > 0,
        "the terminal with output did not draw"
    );
    assert_eq!(view_notifies[1].get(), 0, "the other terminal drew");
    let told: usize = terminal_notifies.iter().map(|count| count.get()).sum();
    assert_eq!(told, 0, "output went through RemoteTerminal");
    assert_eq!(rest.read_with(cx, |rest, _| rest.draws), rest_draws);
}

/// A cached view that draws again draws every cached view inside it. So were the workspace
/// cached, as it once was, the output of one terminal would draw all the cached views in it:
/// the other terminals too.
#[gpui::test]
fn output_under_views_drawn_with_root_draws_no_cached_view_beside_it(cx: &mut TestAppContext) {
    test_views::init(cx);
    let (shell, cx) = cx.add_window_view(|window, cx| {
        let terminals = two_terminals(window, cx);
        let rest = cx.new(|_| Counted { draws: 0 });
        Shell {
            host: cx.new(|_| Host { terminals, rest }),
        }
    });
    settle(cx);
    let host = shell.read_with(cx, |shell, _| shell.host.clone());
    let (terminals, rest) =
        host.read_with(cx, |host, _| (host.terminals.clone(), host.rest.clone()));
    let rest_draws = rest.read_with(cx, |rest, _| rest.draws);
    assert!(
        rest_draws > 0,
        "the cached view beside the terminals never drew"
    );
    let laid_out = layouts();

    let zed = terminals[0].read_with(cx, |terminal, _| terminal.terminal().clone());
    zed.update(cx, |terminal, cx| {
        terminal.write_output(b"built in 2.1 s\n", cx)
    });
    settle(cx);

    assert!(
        layouts() > laid_out,
        "the terminal with output did not lay out its cells"
    );
    assert_eq!(
        rest.read_with(cx, |rest, _| rest.draws),
        rest_draws,
        "a cached view beside the terminal with output drew again"
    );
}
