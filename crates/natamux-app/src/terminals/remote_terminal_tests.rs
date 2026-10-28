//! A snapshot of a terminal that already showed a screen is written to a screen of its own: the
//! old screen stays, with no skeleton, until the new one is parsed (T-0210). Also how the pace,
//! the parser's priority and the focus follow the terminal's state.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui::{
    AnyView, Entity, EntityId, FocusHandle, Focusable, Priority, StyleRefinement, TestAppContext,
    VisualTestContext,
};
use natamux_client::{TerminalEvent, TerminalStatus};
use ui::prelude::*;

use super::{RemoteTerminal, Target};
use crate::app::source::Source;
use crate::app::test_views;
use crate::perf::overlay::Perf;
use crate::terminals::terminal_parser::{PACE_FOCUSED, PACE_HIDDEN, PACE_SHOWN};
use crate::terminals::terminal_screen::terminal_screen;

struct Host {
    terminal: Entity<RemoteTerminal>,
    /// Another view beside the pane, such as a screen's view after a swap.
    extra: Option<AnyView>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .child(terminal_screen(&self.terminal, false, cx))
            .children(self.extra.clone())
    }
}

/// The window's root. `cached`: the host is drawn as a cached view, as the app's workspace is.
struct Root {
    host: Entity<Host>,
    cached: bool,
}

impl Render for Root {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let host = if self.cached {
            let style = StyleRefinement::default().size_full();
            self.host.clone().cached(style).into_any_element()
        } else {
            self.host.clone().into_any_element()
        };
        div().size_full().child(host)
    }
}

/// A shown terminal of the fake daemon, connected: its first layout connects, and the fake's
/// screen comes as live output.
fn open(cx: &mut TestAppContext) -> (Entity<RemoteTerminal>, &mut VisualTestContext) {
    let (terminal, _, cx) = open_in(cx, false);
    (terminal, cx)
}

fn open_in(
    cx: &mut TestAppContext,
    cached: bool,
) -> (Entity<RemoteTerminal>, Entity<Host>, &mut VisualTestContext) {
    test_views::init(cx);
    let source = Arc::new(Source::FakeDaemon);
    let (root, cx) = cx.add_window_view(move |window, cx| {
        let target = Target {
            task_id: "T-0146".into(),
            terminal: "1".into(),
        };
        let terminal = cx.new(|cx| RemoteTerminal::new(source, target, window, cx));
        let host = cx.new(|_| Host {
            terminal,
            extra: None,
        });
        Root { host, cached }
    });
    // Focus listeners see an empty focus path while the window is not active.
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    draw(cx);
    let host = root.read_with(cx, |root, _| root.host.clone());
    let terminal = host.read_with(cx, |host, _| host.terminal.clone());
    (terminal, host, cx)
}

fn apply(
    terminal: &Entity<RemoteTerminal>,
    events: Vec<TerminalEvent>,
    cx: &mut VisualTestContext,
) {
    terminal.update_in(cx, |terminal, window, cx| {
        terminal.apply(events, window, cx)
    });
    cx.run_until_parked();
    draw(cx);
}

/// gpui tells focus listeners while it draws a frame, and a test window draws only when asked.
fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
}

fn output(text: &str, end: Option<u64>) -> TerminalEvent {
    TerminalEvent::Output {
        bytes: text.as_bytes().to_vec(),
        end,
    }
}

fn reset() -> TerminalEvent {
    TerminalEvent::Reset { cols: 80, rows: 24 }
}

fn shown_text(terminal: &Entity<RemoteTerminal>, cx: &mut VisualTestContext) -> String {
    terminal.read_with(cx, |terminal, cx| {
        terminal.terminal().read(cx).get_content()
    })
}

fn view_id(terminal: &Entity<RemoteTerminal>, cx: &mut VisualTestContext) -> EntityId {
    terminal.read_with(cx, |terminal, _| terminal.view.entity_id())
}

fn pace(terminal: &Entity<RemoteTerminal>, cx: &mut VisualTestContext) -> Duration {
    terminal.read_with(cx, |terminal, cx| {
        terminal.terminal().read(cx).wakeup_pace()
    })
}

fn focused(terminal: &Entity<RemoteTerminal>, cx: &mut VisualTestContext) -> bool {
    terminal.read_with(cx, |terminal, _| terminal.focused)
}

fn focus_view(terminal: &Entity<RemoteTerminal>, cx: &mut VisualTestContext) {
    terminal.update_in(cx, |terminal, window, cx| {
        window.focus(&terminal.view.focus_handle(cx), cx)
    });
    cx.run_until_parked();
    draw(cx);
}

fn set_visible(terminal: &Entity<RemoteTerminal>, visible: bool, cx: &mut VisualTestContext) {
    terminal.update(cx, |terminal, cx| terminal.set_visible(visible, cx));
    cx.run_until_parked();
}

/// A later snapshot, parsed: its own screen is swapped in.
fn swap(terminal: &Entity<RemoteTerminal>, cx: &mut VisualTestContext) {
    let first_view = view_id(terminal, cx);
    let events = vec![
        reset(),
        output("qx-new", None),
        TerminalEvent::CaughtUp { offset: 6 },
    ];
    apply(terminal, events, cx);
    assert_ne!(view_id(terminal, cx), first_view, "no swap");
}

fn shows(cx: &mut VisualTestContext) -> usize {
    cx.update(|_, cx| cx.global::<Perf>().stats.shown_count())
}

/// A test window has no frame loop: this delivers the frame's callbacks.
fn next_frame(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.simulate_next_frame(cx));
    cx.run_until_parked();
}

#[gpui::test]
fn a_later_snapshot_keeps_the_old_screen_until_it_is_parsed(cx: &mut TestAppContext) {
    let (terminal, cx) = open(cx);
    let before = shown_text(&terminal, cx);
    assert!(!before.trim().is_empty(), "the fake's screen was not drawn");
    let first_view = view_id(&terminal, cx);

    apply(&terminal, vec![reset(), output("qx-fresh", None)], cx);
    assert!(
        !terminal.read_with(cx, |terminal, _| terminal.replaying()),
        "a terminal that showed a screen got the skeleton"
    );
    assert_eq!(
        shown_text(&terminal, cx),
        before,
        "the old screen went away"
    );

    apply(&terminal, vec![TerminalEvent::CaughtUp { offset: 8 }], cx);
    assert!(shown_text(&terminal, cx).contains("qx-fresh"));
    assert_ne!(
        view_id(&terminal, cx),
        first_view,
        "the snapshot's own view is shown"
    );
}

#[gpui::test]
fn a_first_attach_shows_the_skeleton_until_caught_up(cx: &mut TestAppContext) {
    let (terminal, cx) = open(cx);
    // As a terminal made again after the cache let it go: it drew nothing yet.
    terminal.update(cx, |terminal, _| terminal.drawn = false);
    let first_view = view_id(&terminal, cx);

    apply(&terminal, vec![reset(), output("qx-first", None)], cx);
    assert!(
        terminal.read_with(cx, |terminal, _| terminal.replaying()),
        "no skeleton on a first attach"
    );

    apply(&terminal, vec![TerminalEvent::CaughtUp { offset: 8 }], cx);
    assert!(!terminal.read_with(cx, |terminal, _| terminal.replaying()));
    assert!(shown_text(&terminal, cx).contains("qx-first"));
    assert_eq!(
        view_id(&terminal, cx),
        first_view,
        "a first attach draws on the shown screen"
    );
}

#[gpui::test]
fn output_during_a_snapshot_goes_only_to_its_screen(cx: &mut TestAppContext) {
    let (terminal, cx) = open(cx);
    let old = terminal.read_with(cx, |terminal, _| terminal.terminal().clone());

    let snapshot = vec![reset(), output("qx-a ", None), output("qx-b", None)];
    apply(&terminal, snapshot, cx);
    let caught_up = vec![
        TerminalEvent::CaughtUp { offset: 9 },
        output(" qx-live", Some(17)),
    ];
    apply(&terminal, caught_up, cx);

    let new = shown_text(&terminal, cx);
    for text in ["qx-a", "qx-b", "qx-live"] {
        assert!(new.contains(text), "{text} not on the new screen: {new}");
    }
    let old = old.read_with(cx, |terminal, _| terminal.get_content());
    for text in ["qx-a", "qx-b", "qx-live"] {
        assert!(!old.contains(text), "{text} on the old screen: {old}");
    }
}

#[gpui::test]
fn focus_moves_to_the_new_view_at_the_swap(cx: &mut TestAppContext) {
    let (terminal, cx) = open(cx);
    focus_view(&terminal, cx);
    let first_view = view_id(&terminal, cx);

    let events = vec![
        reset(),
        output("qx", None),
        TerminalEvent::CaughtUp { offset: 2 },
    ];
    apply(&terminal, events, cx);
    assert_ne!(view_id(&terminal, cx), first_view);
    let on_new_view = terminal.update_in(cx, |terminal, window, cx| {
        terminal.view.focus_handle(cx).is_focused(window)
    });
    assert!(on_new_view, "the focus stayed on the old view");
    assert!(focused(&terminal, cx), "the terminal lost its focused lane");
}

/// Regression (T-0210 review 1, finding 2): a second CaughtUp dropped the first swap, and the
/// pane stayed on the old screen while all output went to the new one.
#[gpui::test]
fn a_second_caught_up_keeps_the_waiting_swap(cx: &mut TestAppContext) {
    let (terminal, cx) = open(cx);
    let first_view = view_id(&terminal, cx);

    // One batch: the second CaughtUp comes while the first swap waits for the parser.
    let events = vec![
        reset(),
        output("qx-snapshot", None),
        TerminalEvent::CaughtUp { offset: 11 },
        TerminalEvent::CaughtUp { offset: 11 },
    ];
    apply(&terminal, events, cx);
    assert_ne!(
        view_id(&terminal, cx),
        first_view,
        "the snapshot's screen was dropped"
    );
    assert!(shown_text(&terminal, cx).contains("qx-snapshot"));
}

/// Regression (T-0210 review 1, finding 4): the swapped-in screen kept the pace of when its
/// snapshot started.
#[gpui::test]
fn the_pace_follows_visibility_focus_and_a_swap(cx: &mut TestAppContext) {
    let (terminal, cx) = open(cx);
    assert_eq!(pace(&terminal, cx), PACE_SHOWN);
    set_visible(&terminal, false, cx);
    assert_eq!(pace(&terminal, cx), PACE_HIDDEN);

    // A snapshot starts while hidden; the pane is shown before it is parsed.
    let first_view = view_id(&terminal, cx);
    apply(&terminal, vec![reset(), output("qx", None)], cx);
    set_visible(&terminal, true, cx);
    assert_eq!(pace(&terminal, cx), PACE_SHOWN);
    apply(&terminal, vec![TerminalEvent::CaughtUp { offset: 2 }], cx);
    assert_ne!(view_id(&terminal, cx), first_view);
    assert_eq!(
        pace(&terminal, cx),
        PACE_SHOWN,
        "the new screen kept the hidden pace"
    );

    focus_view(&terminal, cx);
    assert_eq!(pace(&terminal, cx), PACE_FOCUSED);
    // Focus wins while hidden too: a key must echo at once.
    set_visible(&terminal, false, cx);
    assert_eq!(pace(&terminal, cx), PACE_FOCUSED);
}

/// A lane change during a handover: the parser comes back at the newest priority.
#[gpui::test]
fn a_second_lane_change_during_a_handover_takes_the_newest(cx: &mut TestAppContext) {
    let (terminal, cx) = open(cx);
    terminal.update(cx, |terminal, cx| {
        // Shown to hidden starts a handover; focus comes before it ends.
        terminal.set_visible(false, cx);
        assert!(terminal.parser.is_none(), "no handover started");
        terminal.set_focused(true, cx);
    });
    cx.run_until_parked();
    let (spawned_at, parser) = terminal.read_with(cx, |terminal, _| {
        (terminal.spawned_at, terminal.parser.is_some())
    });
    // The priority it was spawned at, not only the one asked for (review 3, finding 2).
    assert_eq!(spawned_at, Priority::High);
    assert!(parser, "the parser did not come back");

    apply(&terminal, vec![output("qx-after", Some(8))], cx);
    assert!(shown_text(&terminal, cx).contains("qx-after"));
}

/// A parent focus node with a child inside it, as a terminal view with its context menu.
struct Nest {
    parent: FocusHandle,
    child: FocusHandle,
}

impl Render for Nest {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .track_focus(&self.parent)
            .child(div().size_full().track_focus(&self.child))
    }
}

struct NestHost {
    terminal: Entity<RemoteTerminal>,
    nest: Entity<Nest>,
}

impl Render for NestHost {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .child(terminal_screen(&self.terminal, false, cx))
            .child(self.nest.clone())
    }
}

/// Regression (T-0210 review 1, finding 6): with `on_blur`, focus in a child of the view (its
/// context menu) cleared the focused lane, and focus back on the view did not count as focus in.
/// The listeners of a screen's view watch a parent node here, so the test owns the child.
#[gpui::test]
fn focus_in_a_child_and_back_keeps_the_terminal_focused(cx: &mut TestAppContext) {
    test_views::init(cx);
    let source = Arc::new(Source::FakeDaemon);
    let (host, cx) = cx.add_window_view(move |window, cx| {
        let target = Target {
            task_id: "T-0146".into(),
            terminal: "1".into(),
        };
        let terminal = cx.new(|cx| RemoteTerminal::new(source, target, window, cx));
        let nest = cx.new(|cx| Nest {
            parent: cx.focus_handle(),
            child: cx.focus_handle(),
        });
        NestHost { terminal, nest }
    });
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    draw(cx);
    let (terminal, nest) = host.read_with(cx, |host, _| (host.terminal.clone(), host.nest.clone()));
    let (parent, child) = nest.read_with(cx, |nest, _| (nest.parent.clone(), nest.child.clone()));
    let _watch = terminal.update_in(cx, |_, window, cx| {
        super::events::watch_focus(&parent, window, cx)
    });
    let focus = |handle: &FocusHandle, cx: &mut VisualTestContext| {
        cx.update(|window, cx| window.focus(handle, cx));
        cx.run_until_parked();
        draw(cx);
    };

    focus(&parent, cx);
    assert!(focused(&terminal, cx));
    focus(&child, cx);
    assert!(
        focused(&terminal, cx),
        "focus in a child counted as focus out"
    );
    focus(&parent, cx);
    assert!(focused(&terminal, cx), "focus back on the parent was lost");
}

/// Regression (T-0210 review 2, finding 1): only the new view was told of a swap. It is not in
/// the drawn tree, so the cached views around the pane kept drawing the old screen. (The old
/// view's notice at the CaughtUp is used up by the frame drawn at the end of that update.) Zed's
/// terminal fills `last_content` only when its element is prepainted: empty means never drawn.
#[gpui::test]
fn a_swap_draws_the_new_screen_inside_a_cached_view(cx: &mut TestAppContext) {
    let (terminal, _, cx) = open_in(cx, true);
    swap(&terminal, cx);
    draw(cx);
    let zed = terminal.read_with(cx, |terminal, _| terminal.terminal().clone());
    let drawn = zed.read_with(cx, |zed, _| !zed.last_content().cells.is_empty());
    assert!(drawn, "the pane still draws the old screen");
}

/// Test-spec 4.3: input on the swapped-in screen reaches the socket, and a swap moves no focus
/// when the old view had none.
#[gpui::test]
fn an_unfocused_swap_gives_the_new_screen_the_input_and_no_focus(cx: &mut TestAppContext) {
    let (terminal, cx) = open(cx);
    swap(&terminal, cx);
    let on_new_view = terminal.update_in(cx, |terminal, window, cx| {
        terminal.view.focus_handle(cx).is_focused(window)
    });
    assert!(
        !on_new_view && !focused(&terminal, cx),
        "the swap moved the focus"
    );

    // The input loop takes the key's time only for input that came through the sink.
    terminal.update(cx, |terminal, _| terminal.last_key = Some(Instant::now()));
    let zed = terminal.read_with(cx, |terminal, _| terminal.terminal().clone());
    zed.update(cx, |zed, _| zed.input(b"x".to_vec()));
    cx.run_until_parked();
    let sent = terminal.read_with(cx, |terminal, _| terminal.last_key.is_none());
    assert!(sent, "input on the new screen did not reach the socket");
}

/// Regression (T-0210 review 3, finding 1): a show ends at its catch-up's end. The daemon
/// answers every show of a parked terminal with a catch-up (empty when nothing was missed), so a
/// frame drawn before it arrives must not end the show. A reconnect clears it.
#[gpui::test]
fn a_show_ends_at_its_catch_up_and_a_reconnect_clears_it(cx: &mut TestAppContext) {
    let (terminal, cx) = open(cx);
    cx.update(|_, cx| cx.global_mut::<Perf>().enabled = true);
    set_visible(&terminal, false, cx);
    set_visible(&terminal, true, cx);
    next_frame(cx);
    assert_eq!(shows(cx), 0, "a frame before the catch-up ended the show");
    // The empty catch-up of a show with nothing missed: no Reset, only its end.
    apply(&terminal, vec![TerminalEvent::CaughtUp { offset: 0 }], cx);
    assert_eq!(shows(cx), 1, "the catch-up did not end the show");
    apply(&terminal, vec![output("qx", Some(2))], cx);
    assert_eq!(shows(cx), 1, "a later output counted again");

    set_visible(&terminal, false, cx);
    set_visible(&terminal, true, cx);
    let reconnecting = TerminalEvent::Status(TerminalStatus::Reconnecting);
    apply(&terminal, vec![reconnecting], cx);
    let shown_at = terminal.read_with(cx, |terminal, _| terminal.shown_at);
    assert!(shown_at.is_none());
    apply(&terminal, vec![TerminalEvent::CaughtUp { offset: 2 }], cx);
    assert_eq!(shows(cx), 1, "a reconnect's catch-up counted as a show");
}

/// Regression (T-0210 review 1, finding 7): a swap must drop the old screen's focus listeners,
/// not keep them. The old view is drawn beside the pane here, so focus on it would count.
#[gpui::test]
fn a_swap_drops_the_old_screens_focus_listeners(cx: &mut TestAppContext) {
    let (terminal, host, cx) = open_in(cx, false);
    let old = terminal.read_with(cx, |terminal, _| terminal.view.clone());
    swap(&terminal, cx);
    host.update(cx, |host, cx| {
        host.extra = Some(old.clone().into());
        cx.notify();
    });
    draw(cx);
    cx.update(|window, cx| window.focus(&old.focus_handle(cx), cx));
    cx.run_until_parked();
    draw(cx);
    assert!(
        !focused(&terminal, cx),
        "the old screen's focus listener still runs"
    );
}
