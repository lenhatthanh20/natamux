//! The terminal matrix of the MIT move (docs/plan-mit-license.md 7.3), recorded on Zed's
//! terminal: what a user does to a terminal (keys, IME text, mouse, wheel, paste, links, focus)
//! and what reaches the shell, read back from `send_input`. The own terminal must pass the same
//! tests. Where the facts are many, a table is kept as a golden file of the `terminal` set.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::cell::Cell;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use gpui::{
    ClipboardEntry, ClipboardItem, Entity, ExternalPaths, FocusHandle, Focusable, Image,
    ImageFormat, KeyDownEvent, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Pixels, Point, ScrollDelta, ScrollWheelEvent, Size, TestAppContext, TouchPhase,
    VisualTestContext, point, size,
};
use natamux_proto::workspaces::files::EntryKind;
use serde_json::{Value, json};
use terminal::{Content, Terminal};
use ui::prelude::*;

use super::{RemoteTerminal, Target, sent};
use crate::app::appearance;
use crate::app::golden_files;
use crate::app::shortcuts::{self, FindInTerminal, MaximizePane};
use crate::app::source::Source;
use crate::app::view_state::ViewState;
use crate::app::{fake_daemon, test_views};
use crate::files::explorer_tree::drop_text;
use crate::terminals::pane_header::{PaneHeader, PaneTitle};
use crate::terminals::terminal_pane::TerminalPane;
use crate::terminals::terminal_screen::terminal_screen;

pub(super) const SET: &str = "terminal";
const TASK: &str = "T-0146";
/// A pane of 80 x 20 cells of the test font (7.8 x 16.9 px), plus the one cell gutter.
pub(super) const PANE: Size<Pixels> = Size {
    width: px(640.),
    height: px(340.),
};

/// The window of a test: the terminal's screen at a fixed size, inside a parent that counts
/// what got past the terminal, and a second focus target to move the focus to.
pub(super) struct Host {
    pub(super) terminal: Entity<RemoteTerminal>,
    pub(super) size: Size<Pixels>,
    /// Key downs that reached the parent: the terminal let them go on.
    pub(super) passed: usize,
    pub(super) wheels: usize,
    pub(super) actions: Vec<&'static str>,
    elsewhere: FocusHandle,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .on_key_down(cx.listener(|host, _: &KeyDownEvent, _, _| host.passed += 1))
            .on_scroll_wheel(cx.listener(|host, _: &ScrollWheelEvent, _, _| host.wheels += 1))
            .on_action(cx.listener(|host, _: &FindInTerminal, _, _| host.actions.push("find")))
            .on_action(cx.listener(|host, _: &MaximizePane, _, _| host.actions.push("maximize")))
            .child(
                div()
                    .w(self.size.width)
                    .h(self.size.height)
                    .child(terminal_screen(&self.terminal, false, cx)),
            )
            .child(div().size(px(10.)).track_focus(&self.elsewhere))
    }
}

/// A window with a terminal of the fake daemon whose screen is empty. Nothing is drawn yet, so
/// the terminal is not connected: `start` does that.
pub(super) fn window(
    cx: &mut TestAppContext,
    pane: Size<Pixels>,
    bind: bool,
) -> (Entity<Host>, Entity<RemoteTerminal>, &mut VisualTestContext) {
    test_views::init(cx);
    if bind {
        cx.update(shortcuts::bind);
    }
    fake_daemon::shared().set_screen(TASK, "1", String::new());
    let source = Arc::new(Source::FakeDaemon);
    let (host, cx) = cx.add_window_view(move |window, cx| {
        let target = Target {
            task_id: TASK.into(),
            terminal: "1".into(),
        };
        let terminal = cx.new(|cx| RemoteTerminal::new(source, target, window, cx));
        Host {
            terminal,
            size: pane,
            passed: 0,
            wheels: 0,
            actions: Vec::new(),
            elsewhere: cx.focus_handle(),
        }
    });
    cx.simulate_resize(size(pane.width + px(40.), pane.height + px(40.)));
    let terminal = host.read_with(cx, |host, _| host.terminal.clone());
    (host, terminal, cx)
}

/// The first layout connects the terminal; then its view takes the focus. What went to the
/// shell until then is dropped.
pub(super) fn start(terminal: &Entity<RemoteTerminal>, cx: &mut VisualTestContext) {
    // Focus listeners see an empty focus path while the window is not active.
    cx.update(|window, _| window.activate_window());
    settle(cx);
    focus(terminal, cx);
    sent::take();
}

pub(super) fn open(
    cx: &mut TestAppContext,
    pane: Size<Pixels>,
    bind: bool,
) -> (Entity<Host>, Entity<RemoteTerminal>, &mut VisualTestContext) {
    let (host, terminal, cx) = window(cx, pane, bind);
    start(&terminal, cx);
    (host, terminal, cx)
}

/// A test window draws only when asked; gpui tells focus listeners while it draws.
pub(super) fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
}

/// Runs what is due, draws, lets the timers of the terminal's pace and the size rule pass, and
/// draws again.
pub(super) fn settle(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    draw(cx);
    cx.executor().advance_clock(Duration::from_millis(300));
    cx.run_until_parked();
    draw(cx);
}

pub(super) fn zed(
    terminal: &Entity<RemoteTerminal>,
    cx: &mut VisualTestContext,
) -> Entity<Terminal> {
    terminal.read_with(cx, |terminal, _| terminal.terminal.clone())
}

/// Output of the program: written to the screen at once, then drawn, so the screen's copy of
/// the modes is current.
pub(super) fn feed(terminal: &Entity<RemoteTerminal>, bytes: &str, cx: &mut VisualTestContext) {
    let screen = zed(terminal, cx);
    screen.update(cx, |screen, cx| screen.write_output(bytes.as_bytes(), cx));
    settle(cx);
}

pub(super) fn focus(terminal: &Entity<RemoteTerminal>, cx: &mut VisualTestContext) {
    terminal.update_in(cx, |terminal, window, cx| {
        window.focus(&terminal.view.focus_handle(cx), cx)
    });
    settle(cx);
}

pub(super) fn blur(host: &Entity<Host>, cx: &mut VisualTestContext) {
    host.update_in(cx, |host, window, cx| window.focus(&host.elsewhere, cx));
    settle(cx);
}

/// All that went to the shell since the last look, joined.
pub(super) fn sent_text() -> String {
    sent::take().concat()
}

/// The screen as a frame drawn now copies it. The view is cached and draws only when told, and
/// typing, for one, tells it nothing until the shell echoes; so the view is told first.
pub(super) fn content<R>(
    terminal: &Entity<RemoteTerminal>,
    cx: &mut VisualTestContext,
    read: impl FnOnce(&Content) -> R,
) -> R {
    let view = terminal.read_with(cx, |terminal, _| terminal.view.clone());
    view.update(cx, |_, cx| cx.notify());
    draw(cx);
    last_drawn(terminal, cx, read)
}

/// The screen as the last frame copied it, with no new frame.
fn last_drawn<R>(
    terminal: &Entity<RemoteTerminal>,
    cx: &mut VisualTestContext,
    read: impl FnOnce(&Content) -> R,
) -> R {
    let screen = zed(terminal, cx);
    screen.read_with(cx, |screen, _| read(screen.last_content()))
}

/// The middle of a cell (line, column), from where the screen laid out its grid.
pub(super) fn cell(
    terminal: &Entity<RemoteTerminal>,
    (line, column): (usize, usize),
    cx: &mut VisualTestContext,
) -> Point<Pixels> {
    content(terminal, cx, |content| {
        let grid = content.terminal_bounds;
        let origin = grid.bounds.origin;
        point(
            origin.x + grid.cell_width * (column as f32 + 0.5),
            origin.y + grid.line_height * (line as f32 + 0.5),
        )
    })
}

pub(super) fn grid(
    terminal: &Entity<RemoteTerminal>,
    cx: &mut VisualTestContext,
) -> (usize, usize) {
    content(terminal, cx, |content| {
        (content.columns, content.screen_lines)
    })
}

pub(super) fn selection(terminal: &Entity<RemoteTerminal>, cx: &mut VisualTestContext) -> Value {
    content(terminal, cx, |content| json!(content.selection_text))
}

fn display_offset(terminal: &Entity<RemoteTerminal>, cx: &mut VisualTestContext) -> usize {
    content(terminal, cx, |content| content.display_offset)
}

pub(super) fn mods(name: &str) -> Modifiers {
    let mut mods = Modifiers::default();
    for part in name
        .split('-')
        .filter(|part| !part.is_empty() && *part != "none")
    {
        match part {
            "shift" => mods.shift = true,
            "ctrl" => mods.control = true,
            "alt" => mods.alt = true,
            "cmd" => mods.platform = true,
            other => panic!("no modifier {other}"),
        }
    }
    mods
}

/// Cmd on macOS, Ctrl elsewhere: the key of links and of the app's chords.
fn secondary() -> &'static str {
    if cfg!(target_os = "macos") {
        "cmd"
    } else {
        "ctrl"
    }
}

/// The app's copy and paste chords in a terminal.
fn copy_chord() -> &'static str {
    if cfg!(target_os = "macos") {
        "cmd-c"
    } else {
        "ctrl-shift-c"
    }
}

fn paste_chord() -> &'static str {
    if cfg!(target_os = "macos") {
        "cmd-v"
    } else {
        "ctrl-shift-v"
    }
}

pub(super) fn mouse_down(
    at: Point<Pixels>,
    button: MouseButton,
    modifiers: Modifiers,
    click_count: usize,
    cx: &mut VisualTestContext,
) {
    cx.simulate_event(MouseDownEvent {
        position: at,
        modifiers,
        button,
        click_count,
        first_mouse: false,
    });
}

pub(super) fn mouse_up(
    at: Point<Pixels>,
    button: MouseButton,
    modifiers: Modifiers,
    click_count: usize,
    cx: &mut VisualTestContext,
) {
    cx.simulate_event(MouseUpEvent {
        position: at,
        modifiers,
        button,
        click_count,
    });
}

pub(super) fn mouse_move(
    at: Point<Pixels>,
    button: Option<MouseButton>,
    modifiers: Modifiers,
    cx: &mut VisualTestContext,
) {
    cx.simulate_event(MouseMoveEvent {
        position: at,
        modifiers,
        pressed_button: button,
    });
}

/// A press and release at one place, the `count`th of a run of clicks.
fn click(
    at: Point<Pixels>,
    button: MouseButton,
    modifiers: Modifiers,
    count: usize,
    cx: &mut VisualTestContext,
) {
    mouse_move(at, None, modifiers, cx);
    mouse_down(at, button, modifiers, count, cx);
    mouse_up(at, button, modifiers, count, cx);
    settle(cx);
}

fn wheel(
    at: Point<Pixels>,
    delta: ScrollDelta,
    modifiers: Modifiers,
    touch_phase: TouchPhase,
    cx: &mut VisualTestContext,
) {
    cx.simulate_event(ScrollWheelEvent {
        position: at,
        delta,
        modifiers,
        touch_phase,
    });
    settle(cx);
}

/// Lines well past one screen, so there is history to scroll.
fn history(lines: usize) -> String {
    (1..=lines).map(|line| format!("line {line}\r\n")).collect()
}

// D35: every text that leaves the app for a shell goes through `send_input`.
#[test]
fn only_send_input_calls_the_input_of_a_terminal_handle() {
    fn walk(dir: &Path, out: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, out);
                continue;
            }
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            if !name.ends_with(".rs") || name.ends_with("_tests.rs") {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap();
            for line in text.lines() {
                let code = line.trim();
                if code.starts_with("//") || !code.contains(".input(") {
                    continue;
                }
                // The stage picker's own `input`, which is no terminal's.
                if code.contains("self.input(cx)") {
                    continue;
                }
                out.push(format!("{name}: {code}"));
            }
        }
    }
    let mut calls = Vec::new();
    walk(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut calls,
    );
    assert_eq!(
        calls,
        ["remote_terminal.rs: handle.input(text);"],
        "a call of a terminal handle's input outside send_input"
    );
}

// T-K2: the app's chords reach the app and nothing reaches the shell; Esc and the shell's own
// chords reach the shell.
#[gpui::test]
fn app_chords_reach_the_app_and_esc_reaches_the_shell(cx: &mut TestAppContext) {
    let (host, terminal, cx) = open(cx, PANE, true);
    feed(&terminal, "copy me\r\n", cx);
    let modifier = secondary();

    cx.simulate_keystrokes(&format!("{modifier}-f"));
    settle(cx);
    assert_eq!(host.read_with(cx, |host, _| host.actions.clone()), ["find"]);
    assert_eq!(sent_text(), "", "Mod+F reached the shell");

    cx.simulate_keystrokes(&format!("{modifier}-shift-enter"));
    settle(cx);
    assert_eq!(
        host.read_with(cx, |host, _| host.actions.clone()),
        ["find", "maximize"]
    );
    assert_eq!(sent_text(), "", "Mod+Shift+Enter reached the shell");

    // Copy with a selection: the clipboard gets it, the shell nothing.
    let at = cell(&terminal, (0, 0), cx);
    click(at, MouseButton::Left, Modifiers::default(), 2, cx);
    cx.simulate_keystrokes(copy_chord());
    settle(cx);
    assert_eq!(
        cx.read_from_clipboard().and_then(|item| item.text()),
        Some("copy".to_string())
    );
    assert_eq!(sent_text(), "", "copy reached the shell");

    cx.write_to_clipboard(ClipboardItem::new_string("pasted".into()));
    cx.simulate_keystrokes(paste_chord());
    settle(cx);
    assert_eq!(sent_text(), "pasted");

    cx.simulate_keystrokes("escape");
    settle(cx);
    assert_eq!(sent_text(), "\x1b", "Esc did not reach the shell");

    // The shell's Ctrl chords: Ctrl+B is the app's Mod+B off macOS, but a terminal keeps it.
    cx.simulate_keystrokes("ctrl-b");
    settle(cx);
    assert_eq!(sent_text(), "\x02");
    assert_eq!(
        host.read_with(cx, |host, _| host.actions.len()),
        2,
        "a shell chord reached the app"
    );
}

// T-I1: committed text goes to the shell once; a key with text is not sent twice.
#[gpui::test]
fn committed_text_is_sent_once(cx: &mut TestAppContext) {
    let (_, _, cx) = open(cx, PANE, false);
    for text in ["người", "日本語", "a"] {
        cx.simulate_input(text);
        settle(cx);
        let sent = sent::take();
        assert_eq!(sent.concat(), text, "{text}: {sent:?}");
        assert_eq!(sent.len(), text.chars().count(), "{text}: {sent:?}");
    }
    cx.simulate_keystrokes("a");
    settle(cx);
    assert_eq!(sent::take(), ["a"]);
}

// T-G1: the grid of a pane, for sizes, scales, font sizes and the scrollbar track, and where the
// spare pixels go (`origin y`) on the main and the alternate screen.
fn grid_rows(cx: &mut TestAppContext) -> Vec<Value> {
    let (host, terminal, cx) = open(cx, PANE, false);
    let mut rows = Vec::new();
    let panes = [(300., 200.), (641., 333.), (1000., 701.), (1234.5, 987.3)];
    for scrollbar in [false, true] {
        let shot = (!scrollbar).then_some("grid");
        cx.update(|_, cx| {
            appearance::init(
                appearance::Inputs {
                    flag: Some(crate::app::options::ThemeMode::Dark),
                    theme_name: None,
                    shot,
                },
                cx,
            )
        });
        for scale in [1., 2.] {
            cx.simulate_scale_factor_change(scale);
            for font in [12., 13., 14., 20.] {
                cx.update(|_, cx| {
                    let view = ViewState {
                        terminal_font: Some(json!({ "size": font })),
                        ..ViewState::default()
                    };
                    appearance::apply(&view, cx);
                });
                for (width, height) in panes {
                    host.update(cx, |host, cx| {
                        host.size = size(px(width), px(height));
                        cx.notify();
                    });
                    cx.simulate_resize(size(px(width + 40.), px(height + 40.)));
                    for screen in ["main", "alt"] {
                        let mode = if screen == "alt" {
                            "\x1b[?1049h"
                        } else {
                            "\x1b[?1049l"
                        };
                        feed(&terminal, mode, cx);
                        let (cols, lines) = grid(&terminal, cx);
                        let (x, y, cw, lh) = content(&terminal, cx, |content| {
                            let grid = content.terminal_bounds;
                            (
                                f32::from(grid.bounds.origin.x),
                                f32::from(grid.bounds.origin.y),
                                f32::from(grid.cell_width),
                                f32::from(grid.line_height),
                            )
                        });
                        rows.push(json!([
                            width, height, scale, font, scrollbar, screen, cols, lines, x, y, cw,
                            lh
                        ]));
                    }
                }
            }
        }
    }
    rows
}

#[gpui::test]
fn the_grid_of_a_pane_matches_the_golden(cx: &mut TestAppContext) {
    golden_files::check(SET, "grid_size", &grid_rows(cx));
}

#[gpui::test]
#[ignore]
fn golden_terminal_grid_size(cx: &mut TestAppContext) {
    golden_files::write(SET, "grid_size", &grid_rows(cx));
}

// T-W1: the wheel and the trackpad: local scroll, wheel reports in mouse mode, arrows on the
// alternate screen, Shift, the phases of a trackpad, and the pane keeps the wheel.
fn wheel_rows(cx: &mut TestAppContext) -> Vec<Value> {
    let (host, terminal, cx) = open(cx, PANE, false);
    let mut rows = Vec::new();
    feed(&terminal, &history(200), cx);
    let at = cell(&terminal, (5, 10), cx);
    let lh = content(&terminal, cx, |content| content.terminal_bounds.line_height);
    let row = |case: &str, terminal: &Entity<RemoteTerminal>, cx: &mut VisualTestContext| {
        let offset = display_offset(terminal, cx);
        let wheels = host.read_with(cx, |host, _| host.wheels);
        json!([case, sent_text(), offset, wheels])
    };
    let none = Modifiers::default();
    let shift = mods("shift");
    let lines = |y: f32| ScrollDelta::Lines(point(0., y));
    for (case, delta, modifiers) in [
        ("lines up 3", lines(3.), none),
        ("lines down 1", lines(-1.), none),
        ("shift lines up 2", lines(2.), shift),
        ("lines down 9", lines(-9.), none),
    ] {
        wheel(at, delta, modifiers, TouchPhase::Moved, cx);
        rows.push(row(case, &terminal, cx));
    }
    // A trackpad: pixels add up within a gesture, and a new gesture starts from 0.
    for (case, y, phase) in [
        ("pixels started", 0., TouchPhase::Started),
        ("pixels 0.6 line", 0.6, TouchPhase::Moved),
        ("pixels 0.6 line again", 0.6, TouchPhase::Moved),
        ("pixels 2.5 lines", 2.5, TouchPhase::Moved),
        ("pixels ended", 0., TouchPhase::Ended),
        ("pixels started again", 0., TouchPhase::Started),
        ("pixels -0.6 line", -0.6, TouchPhase::Moved),
        ("pixels -0.6 line again", -0.6, TouchPhase::Moved),
    ] {
        wheel(
            at,
            ScrollDelta::Pixels(point(px(0.), lh * y)),
            none,
            phase,
            cx,
        );
        rows.push(row(case, &terminal, cx));
    }
    terminal.update(cx, |terminal, cx| terminal.jump_to_latest(cx));
    settle(cx);
    // Mouse mode with SGR reports, then the default reports.
    feed(&terminal, "\x1b[?1000h\x1b[?1006h", cx);
    for (case, delta, modifiers) in [
        ("sgr lines up 2", lines(2.), none),
        ("sgr lines down 1", lines(-1.), none),
        ("sgr shift lines up 1", lines(1.), shift),
        ("sgr ctrl lines up 1", lines(1.), mods("ctrl")),
        ("sgr alt lines down 1", lines(-1.), mods("alt")),
    ] {
        wheel(at, delta, modifiers, TouchPhase::Moved, cx);
        rows.push(row(case, &terminal, cx));
    }
    terminal.update(cx, |terminal, cx| terminal.jump_to_latest(cx));
    feed(&terminal, "\x1b[?1006l", cx);
    wheel(at, lines(1.), none, TouchPhase::Moved, cx);
    rows.push(row("x10 lines up 1", &terminal, cx));
    feed(&terminal, "\x1b[?1000l", cx);
    // The alternate screen: arrows, as alternate scroll is on.
    feed(&terminal, "\x1b[?1049h", cx);
    for (case, delta, modifiers) in [
        ("alt lines up 2", lines(2.), none),
        ("alt lines down 1", lines(-1.), none),
        ("alt shift lines up 1", lines(1.), shift),
    ] {
        wheel(at, delta, modifiers, TouchPhase::Moved, cx);
        rows.push(row(case, &terminal, cx));
    }
    rows
}

#[gpui::test]
fn the_wheel_matches_the_golden(cx: &mut TestAppContext) {
    golden_files::check(SET, "wheel", &wheel_rows(cx));
}

#[gpui::test]
#[ignore]
fn golden_terminal_wheel(cx: &mut TestAppContext) {
    golden_files::write(SET, "wheel", &wheel_rows(cx));
}

/// A terminal pane in a parent that counts the wheel turns that reach it.
struct PaneHost {
    terminal: Entity<RemoteTerminal>,
    wheels: Rc<Cell<usize>>,
}

impl Render for PaneHost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let wheels = self.wheels.clone();
        let header = PaneHeader::new("pane", PaneTitle::Terminal("Terminal 1".into()));
        div()
            .w(px(700.))
            .h(px(400.))
            .flex()
            .flex_col()
            .on_scroll_wheel(move |_, _, _| wheels.set(wheels.get() + 1))
            .child(TerminalPane::new("pane", self.terminal.clone(), header))
    }
}

// T-W1: the pane keeps the wheel, so the grid behind it does not scroll too (T-0162).
#[gpui::test]
fn a_terminal_pane_keeps_the_wheel(cx: &mut TestAppContext) {
    test_views::init(cx);
    let source = Arc::new(Source::FakeDaemon);
    let wheels = Rc::new(Cell::new(0));
    let counted = wheels.clone();
    let (host, cx) = cx.add_window_view(move |window, cx| {
        let target = Target {
            task_id: TASK.into(),
            terminal: "1".into(),
        };
        let terminal = cx.new(|cx| RemoteTerminal::new(source, target, window, cx));
        PaneHost {
            terminal,
            wheels: counted,
        }
    });
    cx.update(|window, _| window.activate_window());
    settle(cx);
    let terminal = host.read_with(cx, |host, _| host.terminal.clone());
    let at = cell(&terminal, (2, 4), cx);
    wheel(
        at,
        ScrollDelta::Lines(point(0., 2.)),
        Modifiers::default(),
        TouchPhase::Moved,
        cx,
    );
    assert_eq!(wheels.get(), 0, "the wheel went on past the pane");
}

// T-S1: selection by click, drag, double and triple click, Shift+click, a right click; copy
// keeps it, typing clears it and scrolls to the bottom.
fn selection_rows(cx: &mut TestAppContext) -> Vec<Value> {
    let (host, terminal, cx) = open(cx, PANE, true);
    let mut rows = Vec::new();
    feed(
        &terminal,
        "alpha beta─gamma│delta epsilon\r\nsecond line here\r\nthird (x) [y] {z} \"q\" 'r'",
        cx,
    );
    let none = Modifiers::default();
    let shift = mods("shift");
    let row = |case: &str, terminal: &Entity<RemoteTerminal>, cx: &mut VisualTestContext| {
        let text = selection(terminal, cx);
        json!([case, text, sent_text()])
    };
    let reset =
        |host: &Entity<Host>, terminal: &Entity<RemoteTerminal>, cx: &mut VisualTestContext| {
            focus(terminal, cx);
            let far = cell(terminal, (10, 70), cx);
            click(far, MouseButton::Left, Modifiers::default(), 1, cx);
            let _ = host;
            sent::take();
        };

    click(cell(&terminal, (0, 2), cx), MouseButton::Left, none, 1, cx);
    rows.push(row("click", &terminal, cx));

    for (case, from, to) in [
        ("drag right", (0, 0), (0, 8)),
        ("drag left", (1, 10), (1, 2)),
        ("drag down two lines", (0, 3), (2, 4)),
    ] {
        reset(&host, &terminal, cx);
        let from = cell(&terminal, from, cx);
        let to = cell(&terminal, to, cx);
        mouse_move(from, None, none, cx);
        mouse_down(from, MouseButton::Left, none, 1, cx);
        mouse_move(to, Some(MouseButton::Left), none, cx);
        mouse_up(to, MouseButton::Left, none, 1, cx);
        settle(cx);
        rows.push(row(case, &terminal, cx));
    }

    for (case, distance) in [("drag 2 px", 2.), ("drag 3 px", 3.)] {
        reset(&host, &terminal, cx);
        let from = cell(&terminal, (1, 3), cx);
        let to = point(from.x + px(distance), from.y);
        mouse_move(from, None, none, cx);
        mouse_down(from, MouseButton::Left, none, 1, cx);
        mouse_move(to, Some(MouseButton::Left), none, cx);
        mouse_up(to, MouseButton::Left, none, 1, cx);
        settle(cx);
        rows.push(row(case, &terminal, cx));
    }

    for (case, at, count) in [
        ("double on beta", (0, 7), 2),
        ("double on gamma", (0, 13), 2),
        ("double on delta", (0, 19), 2),
        ("double in (x)", (2, 7), 2),
        ("double in [y]", (2, 11), 2),
        ("double in \"q\"", (2, 20), 2),
        ("triple on line 2", (1, 4), 3),
        ("four clicks", (1, 4), 4),
    ] {
        reset(&host, &terminal, cx);
        let at = cell(&terminal, at, cx);
        for n in 1..=count {
            mouse_down(at, MouseButton::Left, none, n, cx);
            mouse_up(at, MouseButton::Left, none, n, cx);
        }
        settle(cx);
        rows.push(row(case, &terminal, cx));
    }

    reset(&host, &terminal, cx);
    click(cell(&terminal, (0, 0), cx), MouseButton::Left, none, 1, cx);
    click(cell(&terminal, (1, 5), cx), MouseButton::Left, shift, 1, cx);
    rows.push(row("shift click extends", &terminal, cx));

    reset(&host, &terminal, cx);
    click(cell(&terminal, (1, 6), cx), MouseButton::Left, shift, 1, cx);
    rows.push(row("shift click with no selection", &terminal, cx));

    // A program that reports the mouse: Shift still selects.
    reset(&host, &terminal, cx);
    feed(&terminal, "\x1b[?1000h\x1b[?1006h", cx);
    sent::take();
    let from = cell(&terminal, (0, 0), cx);
    let to = cell(&terminal, (0, 4), cx);
    mouse_move(from, None, shift, cx);
    mouse_down(from, MouseButton::Left, shift, 1, cx);
    mouse_move(to, Some(MouseButton::Left), shift, cx);
    mouse_up(to, MouseButton::Left, shift, 1, cx);
    settle(cx);
    rows.push(row("shift drag in mouse mode", &terminal, cx));
    click(cell(&terminal, (1, 9), cx), MouseButton::Left, shift, 1, cx);
    rows.push(row("shift click in mouse mode", &terminal, cx));
    feed(&terminal, "\x1b[?1000l\x1b[?1006l", cx);

    // A right click selects the word under it when nothing is selected.
    reset(&host, &terminal, cx);
    click(cell(&terminal, (1, 9), cx), MouseButton::Right, none, 1, cx);
    rows.push(row("right click", &terminal, cx));
    focus(&terminal, cx);
    click(cell(&terminal, (0, 2), cx), MouseButton::Right, none, 1, cx);
    rows.push(row("right click with a selection", &terminal, cx));

    // Copy keeps the selection; typing clears it and scrolls to the bottom.
    reset(&host, &terminal, cx);
    let at = cell(&terminal, (1, 1), cx);
    click(at, MouseButton::Left, none, 2, cx);
    cx.simulate_keystrokes(copy_chord());
    settle(cx);
    let copied = cx.read_from_clipboard().and_then(|item| item.text());
    rows.push(json!(["copied", copied]));
    rows.push(row("after copy", &terminal, cx));

    feed(&terminal, &history(100), cx);
    let at = cell(&terminal, (3, 3), cx);
    wheel(
        at,
        ScrollDelta::Lines(point(0., 5.)),
        none,
        TouchPhase::Moved,
        cx,
    );
    click(at, MouseButton::Left, none, 2, cx);
    let before = display_offset(&terminal, cx);
    cx.simulate_keystrokes("x");
    settle(cx);
    let after = display_offset(&terminal, cx);
    rows.push(json!(["typed while scrolled", before, after]));
    rows.push(row("after typing", &terminal, cx));
    rows
}

#[gpui::test]
fn selection_matches_the_golden(cx: &mut TestAppContext) {
    golden_files::check(SET, "selection", &selection_rows(cx));
}

#[gpui::test]
#[ignore]
fn golden_terminal_selection(cx: &mut TestAppContext) {
    golden_files::write(SET, "selection", &selection_rows(cx));
}

// T-V1: Cmd+V of text (bracketed or not), an image and files; a path dropped from the Files
// tab.
fn paste_rows(cx: &mut TestAppContext) -> Vec<Value> {
    let (_, terminal, cx) = open(cx, PANE, true);
    let mut rows = Vec::new();
    let text = "a\r\nb\nc\x1bd\te";
    let image = Image::from_bytes(ImageFormat::Png, vec![0x89, b'P', b'N', b'G']);
    let files = || {
        let paths = ExternalPaths(
            vec![
                "/tmp/a b.txt".into(),
                "/tmp/it's.md".into(),
                "/tmp/plain".into(),
            ]
            .into(),
        );
        ClipboardItem::from(ClipboardEntry::ExternalPaths(paths))
    };
    for (mode, set) in [("off", "\x1b[?2004l"), ("bracketed", "\x1b[?2004h")] {
        feed(&terminal, set, cx);
        for (case, item) in [
            ("text", ClipboardItem::new_string(text.into())),
            ("image", ClipboardItem::new_image(&image)),
            ("files", files()),
            ("empty", ClipboardItem::new_string(String::new())),
        ] {
            cx.write_to_clipboard(item);
            cx.simulate_keystrokes(paste_chord());
            settle(cx);
            rows.push(json!([mode, case, sent_text()]));
        }
        for (case, path, kind) in [
            ("drop file", "src/main.rs", EntryKind::File),
            ("drop folder", "src/my dir", EntryKind::Folder),
        ] {
            let dropped = drop_text(path, kind);
            terminal.update(cx, |terminal, cx| terminal.paste_text(&dropped, cx));
            settle(cx);
            rows.push(json!([mode, case, sent_text()]));
        }
    }
    rows
}

#[gpui::test]
fn paste_matches_the_golden(cx: &mut TestAppContext) {
    golden_files::check(SET, "paste", &paste_rows(cx));
}

#[gpui::test]
#[ignore]
fn golden_terminal_paste(cx: &mut TestAppContext) {
    golden_files::write(SET, "paste", &paste_rows(cx));
}

// T-V1: text typed before the socket opens is sent once, when it opens.
#[gpui::test]
fn text_typed_before_the_socket_opens_is_sent_once_on_open(cx: &mut TestAppContext) {
    let (_, terminal, cx) = window(cx, PANE, false);
    terminal.update(cx, |terminal, _| terminal.type_text("early"));
    cx.update(|window, _| window.activate_window());
    settle(cx);
    assert_eq!(sent::take(), ["early"]);
    terminal.update(cx, |terminal, _| terminal.type_text("late"));
    settle(cx);
    assert_eq!(sent::take(), ["late"]);
}

// T-L1: links. The secondary key shows a link under the pointer; a click with it opens only
// web and mail links; a plain click on an OSC 8 link records today's behaviour (D5).
fn link_rows(cx: &mut TestAppContext) -> Vec<Value> {
    let (_, terminal, cx) = open(cx, size(px(1000.), px(340.)), false);
    let mut rows = Vec::new();
    let lines = [
        "see https://example.com/a). and more",
        "mail mailto:me@example.com, ok",
        "ftp ftp://files.example.com/x;",
        "file file:///etc/hosts done",
        "paren http://example.com/(p) end",
        "path src/main.rs:12 no link",
        "\x1b]8;;https://example.com/osc\x1b\\osc web\x1b]8;;\x1b\\ tail",
        "\x1b]8;;file:///etc/passwd\x1b\\osc file\x1b]8;;\x1b\\ tail",
        "\x1b]8;;ssh://host\x1b\\osc ssh\x1b]8;;\x1b\\ tail",
        "\x1b]8;;HTTPS://EXAMPLE.COM/UP\x1b\\osc upper\x1b]8;;\x1b\\ tail",
    ];
    feed(&terminal, &lines.join("\r\n"), cx);
    let modifier = mods(secondary());
    let opened = |cx: &mut VisualTestContext| cx.opened_url();
    let cases = [
        ("url", (0, 8)),
        ("mailto", (1, 8)),
        ("ftp", (2, 8)),
        ("file", (3, 8)),
        ("paren", (4, 10)),
        ("path", (5, 6)),
        ("osc web", (6, 2)),
        ("osc file", (7, 2)),
        ("osc ssh", (8, 2)),
        ("osc upper", (9, 2)),
    ];
    for (case, at) in cases {
        let at = cell(&terminal, at, cx);
        // Hover with the key down: the link under the pointer, if any.
        cx.simulate_modifiers_change(modifier);
        mouse_move(at, None, modifier, cx);
        settle(cx);
        let hovered = content(&terminal, cx, |content| {
            content
                .last_hovered_word
                .as_ref()
                .map(|word| word.word.clone())
        });
        let before = opened(cx);
        mouse_down(at, MouseButton::Left, modifier, 1, cx);
        mouse_up(at, MouseButton::Left, modifier, 1, cx);
        settle(cx);
        let with_key = opened(cx);
        cx.simulate_modifiers_change(Modifiers::default());
        // A plain click, with no key.
        click(at, MouseButton::Left, Modifiers::default(), 1, cx);
        let plain = opened(cx);
        let changed = |old: &Option<String>, new: &Option<String>| {
            (old != new).then(|| new.clone()).flatten()
        };
        rows.push(json!([
            case,
            hovered,
            changed(&before, &with_key),
            changed(&with_key, &plain),
            sent_text()
        ]));
    }
    rows
}

#[gpui::test]
fn links_match_the_golden(cx: &mut TestAppContext) {
    golden_files::check(SET, "links", &link_rows(cx));
}

#[gpui::test]
#[ignore]
fn golden_terminal_links(cx: &mut TestAppContext) {
    golden_files::write(SET, "links", &link_rows(cx));
}

// T-F1: focus reports only with mode 1004; the cursor of an unfocused view, and of a view whose
// context menu has the focus.
fn focus_rows(cx: &mut TestAppContext) -> Vec<Value> {
    let (host, terminal, cx) = open(cx, PANE, false);
    let mut rows = Vec::new();
    feed(&terminal, "prompt$ ", cx);
    let row = |case: &str, terminal: &Entity<RemoteTerminal>, cx: &mut VisualTestContext| {
        let shape = content(terminal, cx, |content| {
            format!("{:?}", content.cursor.shape)
        });
        let focused = terminal.read_with(cx, |terminal, _| terminal.focused);
        // The view itself, not a child such as its menu.
        let exact = terminal.update_in(cx, |terminal, window, cx| {
            terminal.view.focus_handle(cx).is_focused(window)
        });
        json!([case, sent_text(), shape, focused, exact])
    };
    rows.push(row("focused", &terminal, cx));
    blur(&host, cx);
    rows.push(row("blurred, no mode", &terminal, cx));
    focus(&terminal, cx);
    rows.push(row("focused, no mode", &terminal, cx));
    feed(&terminal, "\x1b[?1004h", cx);
    rows.push(row("mode 1004 on", &terminal, cx));
    blur(&host, cx);
    rows.push(row("blurred, mode 1004", &terminal, cx));
    focus(&terminal, cx);
    rows.push(row("focused, mode 1004", &terminal, cx));
    click(
        cell(&terminal, (0, 2), cx),
        MouseButton::Right,
        Modifiers::default(),
        1,
        cx,
    );
    rows.push(row("context menu open", &terminal, cx));
    focus(&terminal, cx);
    rows.push(row("focused after the menu", &terminal, cx));
    feed(&terminal, "\x1b[?1004l", cx);
    blur(&host, cx);
    rows.push(row("blurred, mode 1004 off", &terminal, cx));
    rows
}

#[gpui::test]
fn focus_matches_the_golden(cx: &mut TestAppContext) {
    golden_files::check(SET, "focus", &focus_rows(cx));
}

#[gpui::test]
#[ignore]
fn golden_terminal_focus(cx: &mut TestAppContext) {
    golden_files::write(SET, "focus", &focus_rows(cx));
}

// T-Q1: the screen answers the program's queries through the shell's input, in order.
fn answer_rows(cx: &mut TestAppContext) -> Vec<Value> {
    let (_, terminal, cx) = open(cx, PANE, false);
    let mut rows = Vec::new();
    for (case, query) in [
        ("DA", "\x1b[c"),
        ("DA2", "\x1b[>c"),
        ("DSR 5", "\x1b[5n"),
        ("DSR 6", "\x1b[3;5H\x1b[6n"),
        ("OSC 10", "\x1b]10;?\x1b\\"),
        ("OSC 11", "\x1b]11;?\x07"),
        ("OSC 12", "\x1b]12;?\x07"),
        ("OSC 4 1", "\x1b]4;1;?\x07"),
        ("OSC 4 200", "\x1b]4;200;?\x07"),
        (
            "OSC 4 set then ask",
            "\x1b]4;3;rgb:12/34/56\x07\x1b]4;3;?\x07",
        ),
        ("pixels", "\x1b[14t"),
        ("cells", "\x1b[18t"),
        ("in order", "\x1b[c\x1b]11;?\x07\x1b[6n\x1b]10;?\x07"),
    ] {
        feed(&terminal, query, cx);
        rows.push(json!([case, sent_text()]));
    }
    rows
}

#[gpui::test]
fn answers_match_the_golden(cx: &mut TestAppContext) {
    golden_files::check(SET, "answers", &answer_rows(cx));
}

#[gpui::test]
#[ignore]
fn golden_terminal_answers(cx: &mut TestAppContext) {
    golden_files::write(SET, "answers", &answer_rows(cx));
}

// T-B1: the cursor blinks only when the program asks, in 500 ms phases; a key shows it; it never
// blinks on the alternate screen.
fn blink_rows(cx: &mut TestAppContext) -> Vec<Value> {
    let (host, terminal, cx) = open(cx, PANE, false);
    let mut rows = Vec::new();
    let shown = |terminal: &Entity<RemoteTerminal>, cx: &mut VisualTestContext| {
        let view = terminal.read_with(cx, |terminal, _| terminal.view.clone());
        view.update_in(cx, |view, window, cx| {
            let focused = view.focus_handle(cx).is_focused(window);
            view.should_show_cursor(focused, cx)
        })
    };
    let mut samples =
        |case: &str, terminal: &Entity<RemoteTerminal>, cx: &mut VisualTestContext| {
            let mut seen = Vec::new();
            for _ in 0..8 {
                seen.push(shown(terminal, cx));
                cx.executor().advance_clock(Duration::from_millis(125));
                cx.run_until_parked();
                draw(cx);
            }
            rows.push(json!([case, seen]));
        };
    samples("default", &terminal, cx);
    feed(&terminal, "\x1b[1 q", cx);
    samples("blinking block", &terminal, cx);
    cx.simulate_keystrokes("a");
    samples("after a key", &terminal, cx);
    feed(&terminal, "\x1b[2 q", cx);
    samples("steady block", &terminal, cx);
    feed(&terminal, "\x1b[?12h", cx);
    samples("mode 12", &terminal, cx);
    feed(&terminal, "\x1b[5 q\x1b[?1049h", cx);
    samples("alt screen, blinking bar", &terminal, cx);
    feed(&terminal, "\x1b[?1049l", cx);
    samples("main screen, blinking bar", &terminal, cx);
    blur(&host, cx);
    samples("unfocused, blinking bar", &terminal, cx);
    focus(&terminal, cx);
    samples("focused again, blinking bar", &terminal, cx);
    sent::take();
    rows
}

#[gpui::test]
fn blink_matches_the_golden(cx: &mut TestAppContext) {
    golden_files::check(SET, "blink", &blink_rows(cx));
}

#[gpui::test]
#[ignore]
fn golden_terminal_blink(cx: &mut TestAppContext) {
    golden_files::write(SET, "blink", &blink_rows(cx));
}

// T-R1: a resize reflows and keeps the text; the history limit stays the one of the first
// width; a change of a few pixels sends nothing.
#[gpui::test]
fn a_resize_keeps_the_text_and_the_history_limit(cx: &mut TestAppContext) {
    let (host, terminal, cx) = open(cx, PANE, false);
    let long: String = (0..120)
        .map(|i| char::from(b'a' + (i % 26) as u8))
        .collect();
    feed(&terminal, &format!("{long}\r\nshort\r\n"), cx);
    let history = terminal.read_with(cx, |terminal, _| terminal.history);
    let (cols, _) = grid(&terminal, cx);
    let size_before = terminal.read_with(cx, |terminal, _| terminal.size);

    let resize = |width: f32, host: &Entity<Host>, cx: &mut VisualTestContext| {
        host.update(cx, |host, cx| {
            host.size.width = px(width);
            cx.notify();
        });
        settle(cx);
    };
    resize(320., &host, cx);
    let (narrow, _) = grid(&terminal, cx);
    assert!(narrow < cols, "the grid did not shrink");
    let text = zed(&terminal, cx).read_with(cx, |screen, _| screen.get_content());
    assert!(
        text.replace('\n', "").contains(&long),
        "the long line was lost: {text}"
    );
    assert_eq!(
        terminal.read_with(cx, |terminal, _| terminal.size),
        (narrow as u16, size_before.1),
        "the shell did not get the new size"
    );
    resize(640., &host, cx);
    let text = zed(&terminal, cx).read_with(cx, |screen, _| screen.get_content());
    assert!(text.contains(&long[..cols.min(120)]), "{text}");
    assert_eq!(
        terminal.read_with(cx, |terminal, _| terminal.history),
        history
    );

    // Less than a cell: the grid and the shell's size stay.
    let before = terminal.read_with(cx, |terminal, _| terminal.size);
    resize(643., &host, cx);
    assert_eq!(terminal.read_with(cx, |terminal, _| terminal.size), before);
    assert_eq!(grid(&terminal, cx).0, cols);
    assert!(!terminal.read_with(cx, |terminal, cx| terminal.layout_changed(cx)));
    assert_eq!(sent_text(), "");
}

// T-R2: a new terminal font draws the cached view again with no new output, and the shell gets
// the new grid size.
#[gpui::test]
fn a_new_terminal_font_draws_again_and_resizes_the_shell(cx: &mut TestAppContext) {
    let (_, terminal, cx) = open(cx, PANE, false);
    feed(&terminal, "text\r\n", cx);
    let (cols, lines) = grid(&terminal, cx);
    assert_eq!(
        terminal.read_with(cx, |terminal, _| terminal.size),
        (cols as u16, lines as u16)
    );
    cx.update(|_, cx| {
        let view = ViewState {
            terminal_font: Some(json!({ "size": 20 })),
            ..ViewState::default()
        };
        appearance::apply(&view, cx);
    });
    settle(cx);
    let (new_cols, new_lines) = last_drawn(&terminal, cx, |content| {
        (content.columns, content.screen_lines)
    });
    assert!(
        new_cols < cols && new_lines < lines,
        "the view did not draw again"
    );
    assert_eq!(
        terminal.read_with(cx, |terminal, _| terminal.size),
        (new_cols as u16, new_lines as u16),
        "the shell did not get the new size"
    );
}

// T-E1: the event loop. With a pace other than 4 ms (the test build of Zed's terminal uses a
// random short delay for 4 ms): the first event after a quiet pace window wakes at once, a burst
// within the pace wakes once after it, a steady stream wakes once per pace window, and a
// writer of a replaced screen wakes nothing.
#[gpui::test]
fn the_event_loop_wakes_once_per_pace_and_drops_a_replaced_screen(cx: &mut TestAppContext) {
    let (_, terminal, cx) = open(cx, PANE, false);
    let screen = zed(&terminal, cx);
    screen.update(cx, |screen, _| {
        screen.set_wakeup_pace(Duration::from_millis(40))
    });
    let wakes = Rc::new(Cell::new(0));
    let counted = wakes.clone();
    cx.update(|_, cx| {
        cx.subscribe(&screen, move |_, event: &terminal::Event, _| {
            if matches!(event, terminal::Event::Wakeup) {
                counted.set(counted.get() + 1);
            }
        })
        .detach();
    });
    let step = |ms: u64, cx: &mut VisualTestContext| {
        cx.executor().advance_clock(Duration::from_millis(ms));
        cx.run_until_parked();
    };
    let mut writer = screen.read_with(cx, |screen, _| screen.display_only_writer().unwrap());
    step(200, cx);
    wakes.set(0);

    writer.write(b"a");
    cx.run_until_parked();
    assert_eq!(wakes.get(), 1, "the first event after quiet waited");
    writer.write(b"b");
    step(5, cx);
    writer.write(b"c");
    step(5, cx);
    assert_eq!(wakes.get(), 1, "a burst woke before the pace ended");
    step(40, cx);
    assert_eq!(wakes.get(), 2, "a burst woke more than once");

    step(200, cx);
    wakes.set(0);
    // 20 writes, 10 ms apart: one at once, then one per 40 ms window up to 200 ms.
    for _ in 0..20 {
        writer.write(b"x");
        step(10, cx);
    }
    step(200, cx);
    assert_eq!(wakes.get(), 6, "a steady stream woke {} times", wakes.get());

    screen
        .update(cx, |screen, cx| screen.replace_display_output(&[], cx))
        .unwrap();
    step(200, cx);
    wakes.set(0);
    writer.write(b"old");
    step(100, cx);
    assert_eq!(
        wakes.get(),
        0,
        "a replaced screen's writer woke the new screen"
    );
    let mut new = screen.read_with(cx, |screen, _| screen.display_only_writer().unwrap());
    new.write(b"new");
    step(100, cx);
    assert_eq!(wakes.get(), 1);
}

// T-E1: a scroll, a resize and a select queued before one frame, and the same one frame each.
// The plan expected equal results. On Zed's terminal Select All in the frame of the resize
// leaves out the 9 lines the resize took from the screen (line 1 to 9), so both results are
// pinned as they are.
#[gpui::test]
fn queued_changes_run_in_their_order_within_one_frame(cx: &mut TestAppContext) {
    let (host, terminal, cx) = open(cx, PANE, false);
    let screen = zed(&terminal, cx);
    let small = size(px(400.), px(200.));
    let state = |terminal: &Entity<RemoteTerminal>, cx: &mut VisualTestContext| {
        content(terminal, cx, |content| {
            json!([
                content.display_offset,
                content.columns,
                content.screen_lines,
                content.selection_text
            ])
        })
    };
    let set_pane = |pane: Size<Pixels>, host: &Entity<Host>, cx: &mut VisualTestContext| {
        host.update(cx, |host, cx| {
            host.size = pane;
            cx.notify();
        });
    };

    // One frame each.
    feed(&terminal, &history(120), cx);
    screen.update(cx, |screen, _| screen.scroll_up_by(5));
    settle(cx);
    set_pane(small, &host, cx);
    settle(cx);
    let bounds = content(&terminal, cx, |content| content.terminal_bounds);
    screen.update(cx, |screen, _| screen.select_all());
    settle(cx);
    let apart = state(&terminal, cx);

    // The same on a new screen, all before one frame.
    set_pane(PANE, &host, cx);
    settle(cx);
    screen
        .update(cx, |screen, cx| screen.replace_display_output(&[], cx))
        .unwrap();
    settle(cx);
    feed(&terminal, &history(120), cx);
    screen.update(cx, |screen, _| {
        screen.scroll_up_by(5);
        screen.set_size(bounds);
        screen.select_all();
    });
    set_pane(small, &host, cx);
    settle(cx);
    let together = state(&terminal, cx);
    let lines = |from: usize| -> String { (from..=120).map(|n| format!("line {n}\n")).collect() };
    assert_eq!(apart, json!([14, 48, 11, lines(1)]));
    assert_eq!(together, json!([14, 48, 11, lines(10)]));
}
