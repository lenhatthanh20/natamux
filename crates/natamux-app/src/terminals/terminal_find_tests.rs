use std::sync::Arc;

use gpui::{Entity, TestAppContext};
use natamux_client::TerminalEvent;
use ui::prelude::*;

use super::TerminalFind;
use crate::app::source::Source;
use crate::app::test_views;
use crate::terminals::remote_terminal::{RemoteTerminal, Target};
use crate::terminals::terminal_screen::terminal_screen;

struct Host {
    terminal: Entity<RemoteTerminal>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .child(terminal_screen(&self.terminal, false, cx))
    }
}

// Review of T-0210: the bar kept the screen it opened on, so after a snapshot swapped in a new
// screen it searched the old one and found nothing.
#[gpui::test]
fn the_bar_searches_the_screen_a_snapshot_swapped_in(cx: &mut TestAppContext) {
    test_views::init(cx);
    let source = Arc::new(Source::FakeDaemon);
    let (host, cx) = cx.add_window_view(move |window, cx| {
        let target = Target {
            task_id: "T-0146".into(),
            terminal: "1".into(),
        };
        let terminal = cx.new(|cx| RemoteTerminal::new(source, target, window, cx));
        Host { terminal }
    });
    cx.run_until_parked();
    let terminal = host.read_with(cx, |host, _| host.terminal.clone());
    let opened_on = terminal.read_with(cx, |terminal, _| terminal.terminal().entity_id());
    let find = host.update_in(cx, |_, window, cx| {
        cx.new(|cx| TerminalFind::new(terminal.clone(), window, cx))
    });

    let snapshot = b"needle in a snapshot".to_vec();
    let offset = snapshot.len() as u64;
    terminal.update_in(cx, |terminal, window, cx| {
        terminal.apply(
            vec![
                TerminalEvent::Reset { cols: 80, rows: 24 },
                TerminalEvent::Output {
                    bytes: snapshot,
                    end: None,
                },
                TerminalEvent::CaughtUp { offset },
            ],
            window,
            cx,
        )
    });
    cx.run_until_parked();
    assert_ne!(
        terminal.read_with(cx, |terminal, _| terminal.terminal().entity_id()),
        opened_on,
        "the snapshot did not swap the screen"
    );

    find.update_in(cx, |find, window, cx| find.set_term("needle", window, cx));
    cx.run_until_parked();
    let found = terminal.read_with(cx, |terminal, cx| terminal.match_count(cx));
    assert_eq!(found, 1);
    assert_eq!(
        find.read_with(cx, |find, _| find.results),
        Some((Some(0), 1))
    );
}
