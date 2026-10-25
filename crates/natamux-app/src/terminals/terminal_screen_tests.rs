//! Output in one terminal draws that terminal's view: the other terminal, the sidebar, the task
//! list and the details are reused, and no Natamux view is told (AC2, test-spec 1.2).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use gpui::{Entity, TestAppContext, VisualTestContext};
use ui::prelude::*;

use super::terminal_screen;
use crate::app::source::Source;
use crate::app::test_views::{self, Counted, cached_style};
use crate::terminals::remote_terminal::{RemoteTerminal, Target};

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
    let source = Arc::new(Source::FakeDaemon);
    let (host, cx) = cx.add_window_view(move |window, cx| {
        let terminals = ["1", "2"]
            .map(|name| {
                let target = Target {
                    task_id: "T-0146".into(),
                    terminal: name.into(),
                };
                let source = Arc::clone(&source);
                cx.new(|cx| RemoteTerminal::new(source, target, window, cx))
            })
            .to_vec();
        Host {
            terminals,
            rest: cx.new(|_| Counted { draws: 0 }),
        }
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
    zed.update(cx, |terminal, cx| terminal.write_output(b"built in 2.1 s\n", cx));
    cx.run_until_parked();

    assert!(view_notifies[0].get() > 0, "the terminal with output did not draw");
    assert_eq!(view_notifies[1].get(), 0, "the other terminal drew");
    let told: usize = terminal_notifies.iter().map(|count| count.get()).sum();
    assert_eq!(told, 0, "output went through RemoteTerminal");
    assert_eq!(rest.read_with(cx, |rest, _| rest.draws), rest_draws);
}
