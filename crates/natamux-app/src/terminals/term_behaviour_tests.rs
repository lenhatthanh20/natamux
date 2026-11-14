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
use ui::prelude::*;

use super::screen::{Content, Event, Terminal};
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
    sent::claims();
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

/// The URL opened since the last look. The test platform keeps only the last URL it opened, so
/// a mark takes its place after each look, and a URL opened twice in a row is seen both times.
pub(super) fn opened(cx: &mut VisualTestContext) -> Option<String> {
    const MARK: &str = "https://mark.invalid/";
    let url = cx.opened_url().filter(|url| url != MARK);
    cx.update(|_, cx| crate::app::links::open_link(MARK, cx));
    url
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
pub(super) fn last_drawn<R>(
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
// spare pixels go (`origin y`) on the main and the alternate screen. The screen has history, so
// a scrollbar exists: a state ending in `-scrollbar` shows it always, any other hides it.
fn grid_rows(cx: &mut TestAppContext) -> Vec<Value> {
    let (host, terminal, cx) = open(cx, PANE, false);
    let mut rows = Vec::new();
    let panes = [(300., 200.), (641., 333.), (1000., 701.), (1234.5, 987.3)];
    feed(&terminal, &history(200), cx);
    for scrollbar in [false, true] {
        let shot = Some(if scrollbar { "grid-scrollbar" } else { "grid" });
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

/// T-G1 on the own grid math. The cell size comes from the font, which the own view measures
/// (S2.4), so here it comes from the table. The bounds are what the old view gave its grid: the
/// pane less the 14 px scrollbar track it always keeps, in whole device pixels as gpui lays out.
#[cfg(feature = "own-terminal")]
#[test]
fn the_own_grid_matches_the_golden() {
    use crate::terminals::term::grid_layout::grid;

    let rows: Vec<Value> = golden_files::read(SET, "grid_size")
        .iter()
        .map(|row| {
            let number = |i: usize| row[i].as_f64().unwrap() as f32;
            let (width, height, scale) = (number(0), number(1), number(2));
            let (cell_width, line_height) = (number(10), number(11));
            let device = |value: f32| px((value * scale).round() / scale);
            let bounds = gpui::Bounds {
                origin: point(px(0.), px(0.)),
                size: size(device(width - 14.), device(height)),
            };
            // History fills the main screen to its last row, so both screens have the spare
            // pixels on top.
            let laid = grid(bounds, px(cell_width), px(line_height), scale, true);
            json!([
                row[0],
                row[1],
                row[2],
                row[3],
                row[4],
                row[5],
                laid.columns,
                laid.lines,
                f32::from(laid.origin.x),
                f32::from(laid.origin.y),
                cell_width,
                line_height
            ])
        })
        .collect();
    golden_files::check(SET, "grid_size", &rows);
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
const LINK_LINES: [&str; 15] = [
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
    "json \"url\":\"https://example.com/j\", ok",
    "angle <https://example.com/b> ok",
    "tick `https://example.com/t` ok",
    "quote 'https://example.com/q' ok",
    "brace {https://example.com/c} ok",
];

/// Each case of T-L1: the cell (line, column) the pointer is on.
const LINK_CASES: [(&str, (usize, usize)); 15] = [
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
    ("json quotes", (10, 16)),
    ("angle brackets", (11, 12)),
    ("backticks", (12, 10)),
    ("single quotes", (13, 12)),
    ("braces", (14, 12)),
];

fn link_rows(cx: &mut TestAppContext) -> Vec<Value> {
    let (_, terminal, cx) = open(cx, size(px(1000.), px(340.)), false);
    let mut rows = Vec::new();
    feed(&terminal, &LINK_LINES.join("\r\n"), cx);
    let modifier = mods(secondary());
    for (case, at) in LINK_CASES {
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
        opened(cx);
        mouse_down(at, MouseButton::Left, modifier, 1, cx);
        mouse_up(at, MouseButton::Left, modifier, 1, cx);
        settle(cx);
        let with_key = opened(cx);
        cx.simulate_modifiers_change(Modifiers::default());
        // A plain click, with no key.
        click(at, MouseButton::Left, Modifiers::default(), 1, cx);
        let plain = opened(cx);
        rows.push(json!([case, hovered, with_key, plain, sent_text()]));
    }
    rows
}

#[gpui::test]
fn links_match_the_golden(cx: &mut TestAppContext) {
    let rows = link_rows(cx);
    #[cfg(not(feature = "own-terminal"))]
    golden_files::check(SET, "links", &rows);
    #[cfg(feature = "own-terminal")]
    assert_eq!(rows, safe_plain_clicks(golden_files::read(SET, "links")));
}

/// The old rows as the own terminal gives them (D5): a plain click on an OSC 8 cell opens only a
/// web or mail link, so the other targets of the old plain clicks open nothing.
#[cfg(feature = "own-terminal")]
fn safe_plain_clicks(golden: Vec<Value>) -> Vec<Value> {
    use crate::terminals::term::links::may_open;

    golden
        .into_iter()
        .map(|mut row| {
            if row[3].as_str().is_some_and(|url| !may_open(url)) {
                row[3] = Value::Null;
            }
            row
        })
        .collect()
}

/// T-L1 on the own link rules: the link the own model finds under the pointer (an OSC 8 link
/// from its cells, else a URL of the line), shown with the secondary key, and what a click with
/// the key opens. A plain click is the view's (S2.4, D5).
#[cfg(feature = "own-terminal")]
#[gpui::test]
fn the_own_links_match_the_golden(cx: &mut TestAppContext) {
    use alacritty_terminal::index::{Column, Line, Point};

    use crate::terminals::term::links::{may_open, shown};
    use crate::terminals::term::model::tests::{feed, screen};

    let golden = golden_files::read(SET, "links");
    let mut own = screen(cx, (128, 20));
    feed(&mut own, &LINK_LINES.join("\r\n"), cx);
    let mut rows = Vec::new();
    for (case, (line, column)) in LINK_CASES {
        let at = Point::new(Line(line as i32), Column(column));
        let target = own
            .model
            .read_with(cx, |model, _| model.link_at(at))
            .map(|link| link.word);
        let with_key = target.clone().filter(|url| may_open(url));
        rows.push(json!([case, target.as_deref().map(shown), with_key]));
    }
    let golden: Vec<Value> = golden
        .iter()
        .map(|row| json!(row.as_array().unwrap()[..3]))
        .collect();
    assert_eq!(rows, golden);
}

#[gpui::test]
#[ignore]
fn golden_terminal_links(cx: &mut TestAppContext) {
    golden_files::write(SET, "links", &link_rows(cx));
}

// T-M1 and T-L1, more rows (S2.4 review): mouse reports while the screen is scrolled up, the
// secondary key on a link while a program takes the mouse, a drag that leaves the pane, a move
// across the middle of a cell, a move after a release, the middle button, and OSC 8 releases
// with Shift or a second click.
const POINTER_LINES: [&str; 10] = [
    "web https://example.com/m1 tail",
    "\x1b]8;;https://example.com/m2\x1b\\osc two\x1b]8;;\x1b\\ tail",
    "\x1b]8;;https://example.com/m3\x1b\\osc three\x1b]8;;\x1b\\ tail",
    "\x1b]8;;https://example.com/m4\x1b\\osc four\x1b]8;;\x1b\\ tail",
    "web https://example.com/m5 tail",
    "abc def ghi",
    "web https://example.com/m6 tail",
    "\x1b]8;;https://example.com/m7\x1b\\osc seven\x1b]8;;\x1b\\ tail",
    "\x1b]8;;https://example.com/m8\x1b\\osc eight\x1b]8;;\x1b\\ tail",
    "\x1b]8;;https://example.com/m9\x1b\\osc nine\x1b]8;;\x1b\\ tail",
];

/// The primary selection's text, on the systems that have one.
fn primary(cx: &mut VisualTestContext) -> Value {
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    let text = cx.update(|_, cx| cx.read_from_primary().and_then(|item| item.text()));
    #[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
    let text: Option<String> = {
        let _ = cx;
        None
    };
    json!(text)
}

fn view_focused(terminal: &Entity<RemoteTerminal>, cx: &mut VisualTestContext) -> bool {
    terminal.update_in(cx, |terminal, window, cx| {
        terminal.view.focus_handle(cx).is_focused(window)
    })
}

fn pointer_rows(cx: &mut TestAppContext) -> Vec<Value> {
    let (host, terminal, cx) = open(cx, PANE, false);
    let mut rows = Vec::new();
    let none = Modifiers::default();
    let shift = mods("shift");
    let key = mods(secondary());
    let left = Some(MouseButton::Left);
    feed(&terminal, &history(60), cx);
    // The lines go on the top rows, over the screen the history left.
    feed(
        &terminal,
        &format!("\x1b[H{}\x1b[K", POINTER_LINES.join("\x1b[K\r\n")),
        cx,
    );
    let lh = content(&terminal, cx, |content| content.terminal_bounds.line_height);
    let cw = content(&terminal, cx, |content| content.terminal_bounds.cell_width);
    let sgr = |mode: &str| format!("\x1b[?{mode}h\x1b[?1006h");
    let off = "\x1b[?1000l\x1b[?1002l\x1b[?1003l\x1b[?1006l";
    let row = |case: &str, terminal: &Entity<RemoteTerminal>, cx: &mut VisualTestContext| {
        let offset = display_offset(terminal, cx);
        json!([case, sent_text(), offset])
    };
    // Mouse mode on the main screen, scrolled up 5 lines with Shift.
    feed(&terminal, &sgr("1000"), cx);
    sent_text();
    wheel(
        cell(&terminal, (10, 10), cx),
        ScrollDelta::Lines(point(0., 5.)),
        shift,
        TouchPhase::Moved,
        cx,
    );
    rows.push(row("scrolled up", &terminal, cx));
    click(cell(&terminal, (2, 4), cx), MouseButton::Left, none, 1, cx);
    rows.push(row("click above the offset", &terminal, cx));
    click(cell(&terminal, (8, 4), cx), MouseButton::Left, none, 1, cx);
    rows.push(row("click below the offset", &terminal, cx));
    let lines = |y: f32| ScrollDelta::Lines(point(0., y));
    wheel(
        cell(&terminal, (8, 4), cx),
        lines(-1.),
        none,
        TouchPhase::Moved,
        cx,
    );
    rows.push(row("wheel below the offset", &terminal, cx));
    wheel(
        cell(&terminal, (2, 4), cx),
        lines(1.),
        none,
        TouchPhase::Moved,
        cx,
    );
    rows.push(row("wheel above the offset", &terminal, cx));
    terminal.update(cx, |terminal, cx| terminal.jump_to_latest(cx));
    settle(cx);
    sent_text();

    // The secondary key on a link while the program takes the mouse.
    let at = cell(&terminal, (0, 8), cx);
    cx.simulate_modifiers_change(key);
    mouse_move(at, None, key, cx);
    settle(cx);
    sent_text();
    mouse_down(at, MouseButton::Left, key, 1, cx);
    mouse_up(at, MouseButton::Left, key, 1, cx);
    settle(cx);
    rows.push(json!([
        "key click on a hovered url",
        sent_text(),
        opened(cx)
    ]));
    cx.simulate_modifiers_change(none);
    let at = cell(&terminal, (1, 2), cx);
    mouse_move(at, None, none, cx);
    settle(cx);
    sent_text();
    mouse_down(at, MouseButton::Left, key, 1, cx);
    mouse_up(at, MouseButton::Left, key, 1, cx);
    settle(cx);
    rows.push(json!(["key click on an osc link", sent_text(), opened(cx)]));
    feed(&terminal, &sgr("1002"), cx);
    sent_text();
    let (from, to) = (cell(&terminal, (2, 1), cx), cell(&terminal, (2, 4), cx));
    mouse_move(from, None, none, cx);
    mouse_down(from, MouseButton::Left, key, 1, cx);
    mouse_move(to, left, key, cx);
    mouse_up(to, MouseButton::Left, key, 1, cx);
    settle(cx);
    rows.push(json!(["key drag on a link", sent_text(), opened(cx)]));
    let (from, to) = (cell(&terminal, (3, 2), cx), cell(&terminal, (12, 10), cx));
    mouse_move(from, None, none, cx);
    mouse_down(from, MouseButton::Left, key, 1, cx);
    mouse_move(to, left, key, cx);
    mouse_up(to, MouseButton::Left, key, 1, cx);
    settle(cx);
    rows.push(json!([
        "key press on a link, release off it",
        sent_text(),
        opened(cx)
    ]));
    let at = cell(&terminal, (4, 8), cx);
    mouse_move(at, None, none, cx);
    mouse_down(at, MouseButton::Left, key, 1, cx);
    mouse_up(at, MouseButton::Left, none, 1, cx);
    settle(cx);
    rows.push(json!([
        "key press on a link, release with no key",
        sent_text(),
        opened(cx)
    ]));
    let at = cell(&terminal, (12, 10), cx);
    mouse_move(at, None, none, cx);
    settle(cx);
    sent_text();
    mouse_down(at, MouseButton::Left, key, 1, cx);
    mouse_up(at, MouseButton::Left, key, 1, cx);
    settle(cx);
    rows.push(json!(["key click off a link", sent_text(), opened(cx)]));
    feed(&terminal, off, cx);
    let at = cell(&terminal, (6, 8), cx);
    mouse_move(at, None, none, cx);
    settle(cx);
    mouse_down(at, MouseButton::Left, key, 1, cx);
    mouse_up(at, MouseButton::Left, key, 1, cx);
    settle(cx);
    rows.push(json!([
        "no mouse mode, key click on a url",
        sent_text(),
        opened(cx)
    ]));
    focus(&terminal, cx);
    sent_text();

    // A drag that leaves the pane, in mode 1002.
    feed(&terminal, &sgr("1002"), cx);
    sent_text();
    let at = cell(&terminal, (14, 5), cx);
    mouse_move(at, None, none, cx);
    mouse_down(at, MouseButton::Left, none, 1, cx);
    settle(cx);
    rows.push(row("press", &terminal, cx));
    mouse_move(cell(&terminal, (15, 6), cx), left, none, cx);
    settle(cx);
    rows.push(row("drag in the pane", &terminal, cx));
    let outside = point(cell(&terminal, (15, 6), cx).x, PANE.height + px(10.));
    mouse_move(outside, left, none, cx);
    settle(cx);
    rows.push(row("drag out of the pane", &terminal, cx));
    let further = point(outside.x + cw * 3., outside.y);
    mouse_move(further, left, none, cx);
    settle(cx);
    rows.push(row("drag outside the pane", &terminal, cx));
    mouse_up(further, MouseButton::Left, none, 1, cx);
    settle(cx);
    rows.push(row("release outside the pane", &terminal, cx));
    focus(&terminal, cx);
    sent_text();

    // Every move in mode 1003: across the middle of a cell, and after a release in one cell.
    feed(&terminal, &sgr("1003"), cx);
    sent_text();
    let middle = cell(&terminal, (16, 7), cx);
    for (case, dx) in [
        ("move to the left half", -0.3),
        ("move to the right half", 0.3),
        ("move in the right half", 0.4),
    ] {
        mouse_move(point(middle.x + cw * dx, middle.y), None, none, cx);
        settle(cx);
        rows.push(row(case, &terminal, cx));
    }
    let at = cell(&terminal, (17, 9), cx);
    mouse_move(at, None, none, cx);
    settle(cx);
    rows.push(row("move to a cell", &terminal, cx));
    mouse_down(at, MouseButton::Left, none, 1, cx);
    mouse_up(at, MouseButton::Left, none, 1, cx);
    settle(cx);
    rows.push(row("click in it", &terminal, cx));
    mouse_move(point(at.x, at.y + lh * 0.1), None, none, cx);
    settle(cx);
    rows.push(row("move in it after the release", &terminal, cx));
    feed(&terminal, off, cx);
    sent_text();

    // The middle button: it focuses the view, and on Linux pastes the primary selection when
    // the press is not a report.
    let (from, to) = (cell(&terminal, (5, 0), cx), cell(&terminal, (5, 2), cx));
    mouse_move(from, None, none, cx);
    mouse_down(from, MouseButton::Left, none, 1, cx);
    mouse_move(to, left, none, cx);
    mouse_up(to, MouseButton::Left, none, 1, cx);
    settle(cx);
    rows.push(json!(["drag over abc", primary(cx)]));
    blur(&host, cx);
    click(
        cell(&terminal, (12, 3), cx),
        MouseButton::Middle,
        none,
        1,
        cx,
    );
    let focused = view_focused(&terminal, cx);
    rows.push(json!(["middle click", sent_text(), focused]));
    feed(&terminal, &sgr("1000"), cx);
    sent_text();
    let (from, to) = (cell(&terminal, (5, 4), cx), cell(&terminal, (5, 6), cx));
    mouse_move(from, None, shift, cx);
    mouse_down(from, MouseButton::Left, shift, 1, cx);
    mouse_move(to, left, shift, cx);
    mouse_up(to, MouseButton::Left, shift, 1, cx);
    settle(cx);
    rows.push(json!(["shift drag over def", primary(cx), sent_text()]));
    click(
        cell(&terminal, (12, 3), cx),
        MouseButton::Middle,
        shift,
        1,
        cx,
    );
    rows.push(json!(["shift middle click in mouse mode", sent_text()]));
    let (from, to) = (cell(&terminal, (14, 30), cx), cell(&terminal, (15, 32), cx));
    mouse_move(from, None, shift, cx);
    mouse_down(from, MouseButton::Left, shift, 1, cx);
    mouse_move(to, left, shift, cx);
    mouse_up(to, MouseButton::Left, shift, 1, cx);
    settle(cx);
    rows.push(json!([
        "shift drag over blank cells",
        primary(cx),
        sent_text()
    ]));
    click(
        cell(&terminal, (12, 3), cx),
        MouseButton::Middle,
        shift,
        1,
        cx,
    );
    rows.push(json!(["shift middle click after blank cells", sent_text()]));
    blur(&host, cx);
    click(
        cell(&terminal, (12, 3), cx),
        MouseButton::Middle,
        none,
        1,
        cx,
    );
    let focused = view_focused(&terminal, cx);
    rows.push(json!(["middle click in mouse mode", sent_text(), focused]));
    feed(&terminal, off, cx);
    sent_text();

    // Releases on an OSC 8 link: a second click, and a click with Shift. Above the middle of
    // the cell, so the rounding of a plain click stays on its line.
    let upper = |line: usize, cx: &mut VisualTestContext| {
        let at = cell(&terminal, (line, 2), cx);
        point(at.x, at.y - lh * 0.25)
    };
    let at = upper(7, cx);
    mouse_move(at, None, none, cx);
    mouse_down(at, MouseButton::Left, none, 2, cx);
    mouse_up(at, MouseButton::Left, none, 2, cx);
    settle(cx);
    rows.push(json!([
        "second click on an osc link",
        sent_text(),
        opened(cx)
    ]));
    let at = upper(8, cx);
    mouse_move(at, None, shift, cx);
    mouse_down(at, MouseButton::Left, shift, 1, cx);
    mouse_up(at, MouseButton::Left, shift, 1, cx);
    settle(cx);
    rows.push(json!([
        "shift click on an osc link",
        sent_text(),
        opened(cx)
    ]));
    click(upper(9, cx), MouseButton::Left, none, 1, cx);
    rows.push(json!([
        "plain click on an osc link",
        sent_text(),
        opened(cx)
    ]));

    // Presses in the scrollbar's track, away from its thumb (at the bottom: the newest lines
    // show), outside mouse mode and in mode 1000. Then releases off the screen after a press
    // on a link of the last column.
    // Each link ends in the last column.
    feed(
        &terminal,
        concat!(
            "\x1b[H\x1b[2J\x1b[1;999H\x1b[20Dhttps://example.com/e",
            "\x1b[2;999H\x1b[9D\x1b]8;;https://example.com/o\x1b\\osc edge 1\x1b]8;;\x1b\\",
            "\x1b[3;1Habc def ghi",
        ),
        cx,
    );
    let last = grid(&terminal, cx).0 - 1;
    let in_track = |line: usize, cx: &mut VisualTestContext| {
        point(PANE.width - px(5.), cell(&terminal, (line, 0), cx).y)
    };
    let track_row = |case: &str, terminal: &Entity<RemoteTerminal>, cx: &mut VisualTestContext| {
        let text = selection(terminal, cx);
        let offset = display_offset(terminal, cx);
        let focused = view_focused(terminal, cx);
        json!([case, sent_text(), text, offset, focused])
    };
    let select_abc = |cx: &mut VisualTestContext| {
        let (from, to) = (cell(&terminal, (2, 0), cx), cell(&terminal, (2, 2), cx));
        mouse_move(from, None, none, cx);
        mouse_down(from, MouseButton::Left, none, 1, cx);
        mouse_move(to, left, none, cx);
        mouse_up(to, MouseButton::Left, none, 1, cx);
        settle(cx);
    };
    select_abc(cx);
    rows.push(track_row("select abc", &terminal, cx));
    blur(&host, cx);
    click(in_track(4, cx), MouseButton::Left, none, 1, cx);
    rows.push(track_row("click in the track", &terminal, cx));
    focus(&terminal, cx);
    click(in_track(4, cx), MouseButton::Left, shift, 1, cx);
    rows.push(track_row("shift click in the track", &terminal, cx));
    click(in_track(4, cx), MouseButton::Left, none, 2, cx);
    rows.push(track_row("double click in the track", &terminal, cx));
    let at = in_track(4, cx);
    mouse_move(at, None, none, cx);
    mouse_down(at, MouseButton::Left, none, 1, cx);
    mouse_move(cell(&terminal, (5, 4), cx), left, none, cx);
    mouse_move(cell(&terminal, (6, 8), cx), left, none, cx);
    mouse_up(cell(&terminal, (6, 8), cx), MouseButton::Left, none, 1, cx);
    settle(cx);
    rows.push(track_row(
        "press in the track, drag over the screen",
        &terminal,
        cx,
    ));
    select_abc(cx);
    blur(&host, cx);
    click(in_track(4, cx), MouseButton::Middle, none, 1, cx);
    rows.push(track_row("middle click in the track", &terminal, cx));
    focus(&terminal, cx);
    feed(&terminal, &sgr("1000"), cx);
    sent_text();
    blur(&host, cx);
    click(in_track(4, cx), MouseButton::Left, none, 1, cx);
    rows.push(track_row("click in the track in mouse mode", &terminal, cx));
    focus(&terminal, cx);
    let at = in_track(4, cx);
    mouse_move(at, None, none, cx);
    mouse_down(at, MouseButton::Left, none, 1, cx);
    mouse_move(cell(&terminal, (5, 4), cx), left, none, cx);
    mouse_up(cell(&terminal, (5, 4), cx), MouseButton::Left, none, 1, cx);
    settle(cx);
    rows.push(track_row(
        "press in the track in mouse mode, release over the screen",
        &terminal,
        cx,
    ));
    feed(&terminal, off, cx);
    sent_text();

    let outside = |line: usize, cx: &mut VisualTestContext| {
        point(PANE.width + px(10.), cell(&terminal, (line, 0), cx).y)
    };
    let upper = |line: usize, column: usize, cx: &mut VisualTestContext| {
        let at = cell(&terminal, (line, column), cx);
        point(at.x, at.y - lh * 0.25)
    };
    for (case, to) in [
        (
            "key press on an edge url, release in the track",
            in_track(0, cx),
        ),
        ("key press on an edge url, release outside", outside(0, cx)),
    ] {
        let at = cell(&terminal, (0, last - 3), cx);
        cx.simulate_modifiers_change(key);
        mouse_move(at, None, key, cx);
        settle(cx);
        mouse_down(at, MouseButton::Left, key, 1, cx);
        mouse_move(to, left, key, cx);
        mouse_up(to, MouseButton::Left, key, 1, cx);
        settle(cx);
        cx.simulate_modifiers_change(none);
        rows.push(json!([case, sent_text(), opened(cx)]));
    }
    for (case, to) in [
        (
            "plain press on an edge osc link, release in the track",
            in_track(1, cx),
        ),
        (
            "plain press on an edge osc link, release outside",
            outside(1, cx),
        ),
    ] {
        let at = upper(1, last - 3, cx);
        mouse_move(at, None, none, cx);
        mouse_down(at, MouseButton::Left, none, 1, cx);
        let to = point(to.x, at.y);
        mouse_move(to, left, none, cx);
        mouse_up(to, MouseButton::Left, none, 1, cx);
        settle(cx);
        rows.push(json!([case, sent_text(), opened(cx)]));
    }
    rows
}

#[gpui::test]
fn pointer_edges_match_the_golden(cx: &mut TestAppContext) {
    golden_files::check(SET, "pointer_edges", &pointer_rows(cx));
}

#[gpui::test]
#[ignore]
fn golden_terminal_pointer_edges(cx: &mut TestAppContext) {
    golden_files::write(SET, "pointer_edges", &pointer_rows(cx));
}

// T-S1, more rows (S2.4 review): drags past the right edge, below the last row, and out of the
// screen above and below, where the screen scrolls.
fn selection_edge_rows(cx: &mut TestAppContext) -> Vec<Value> {
    let (_, terminal, cx) = open(cx, PANE, false);
    let mut rows = Vec::new();
    let none = Modifiers::default();
    let left = Some(MouseButton::Left);
    let full: String = (0..80).map(|i| char::from(b'a' + (i % 26) as u8)).collect();
    feed(&terminal, &history(60), cx);
    // A full line on every row, each ending in its row number, so a selection names its rows.
    let rows_text: Vec<String> = (0..20)
        .map(|row| format!("{}{row:02}", &full[..78]))
        .collect();
    feed(&terminal, &format!("\x1b[H{}", rows_text.join("\r\n")), cx);
    let grid = content(&terminal, cx, |content| content.terminal_bounds);
    let (cw, lh) = (grid.cell_width, grid.line_height);
    let top = grid.bounds.origin.y;
    let right = grid.bounds.origin.x + cw * 80.;
    let bottom = top + lh * 20.;
    let row = |case: &str, terminal: &Entity<RemoteTerminal>, cx: &mut VisualTestContext| {
        let text = selection(terminal, cx);
        let offset = display_offset(terminal, cx);
        json!([case, text, offset])
    };
    let drag = |from: Point<Pixels>, to: &[Point<Pixels>], cx: &mut VisualTestContext| {
        mouse_move(from, None, none, cx);
        mouse_down(from, MouseButton::Left, none, 1, cx);
        for at in to {
            mouse_move(*at, left, none, cx);
        }
        mouse_up(*to.last().unwrap(), MouseButton::Left, none, 1, cx);
        settle(cx);
    };
    let reset = |terminal: &Entity<RemoteTerminal>, cx: &mut VisualTestContext| {
        terminal.update(cx, |terminal, cx| terminal.jump_to_latest(cx));
        settle(cx);
        click(cell(terminal, (10, 40), cx), MouseButton::Left, none, 1, cx);
    };
    let y0 = cell(&terminal, (0, 0), cx).y;
    for (case, x) in [
        ("drag past the right edge", right + px(3.)),
        ("drag out of the pane to the right", PANE.width + px(10.)),
    ] {
        reset(&terminal, cx);
        drag(cell(&terminal, (0, 70), cx), &[point(x, y0)], cx);
        rows.push(row(case, &terminal, cx));
    }
    let x4 = cell(&terminal, (19, 4), cx).x - cw * 0.3;
    for (case, y) in [
        ("drag below the last row", bottom + px(1.)),
        ("drag out of the pane below", PANE.height + px(10.)),
    ] {
        reset(&terminal, cx);
        drag(cell(&terminal, (17, 0), cx), &[point(x4, y)], cx);
        rows.push(row(case, &terminal, cx));
    }
    let x6 = cell(&terminal, (0, 6), cx).x;
    for (case, ys) in [
        ("drag above the top, 5 px", vec![top - px(5.)]),
        ("drag above the top, 25 px", vec![top - px(25.)]),
        (
            "drag above the top, two moves",
            vec![top - px(5.), top - px(6.)],
        ),
    ] {
        reset(&terminal, cx);
        let to: Vec<_> = ys.into_iter().map(|y| point(x6, y)).collect();
        drag(cell(&terminal, (5, 3), cx), &to, cx);
        rows.push(row(case, &terminal, cx));
    }
    reset(&terminal, cx);
    wheel(
        cell(&terminal, (10, 10), cx),
        ScrollDelta::Lines(point(0., 10.)),
        none,
        TouchPhase::Moved,
        cx,
    );
    let below = [point(x6, bottom + px(5.)), point(x6, bottom + px(6.))];
    drag(cell(&terminal, (5, 3), cx), &below, cx);
    rows.push(row(
        "drag below the bottom while scrolled up",
        &terminal,
        cx,
    ));
    // The same drags with a first move inside the screen, released outside or back inside.
    let y5 = cell(&terminal, (5, 0), cx).y;
    let inside = |line: usize, column: usize, cx: &mut VisualTestContext| {
        cell(&terminal, (line, column), cx)
    };
    let cases = [
        (
            "drag in, then past the right edge",
            (0, 70),
            vec![inside(0, 75, cx), point(right + px(3.), y0)],
        ),
        (
            "drag in, then past the right edge and back",
            (0, 70),
            vec![
                inside(0, 75, cx),
                point(right + px(3.), y0),
                inside(0, 77, cx),
            ],
        ),
        (
            "drag in, then above the top",
            (5, 3),
            vec![inside(2, 6, cx), point(x6, top - px(5.))],
        ),
        (
            "drag in, then above the top and back",
            (5, 3),
            vec![inside(2, 6, cx), point(x6, top - px(5.)), inside(1, 6, cx)],
        ),
        (
            "drag in, then below the bottom",
            (5, 3),
            vec![inside(10, 6, cx), point(x6, PANE.height + px(10.))],
        ),
        (
            "drag in, then below the bottom and back",
            (5, 3),
            vec![
                inside(10, 6, cx),
                point(x6, PANE.height + px(10.)),
                point(x6, y5),
            ],
        ),
    ];
    for (case, from, to) in cases {
        reset(&terminal, cx);
        drag(cell(&terminal, from, cx), &to, cx);
        rows.push(row(case, &terminal, cx));
    }
    rows
}

#[gpui::test]
fn selection_edges_match_the_golden(cx: &mut TestAppContext) {
    golden_files::check(SET, "selection_edges", &selection_edge_rows(cx));
}

#[gpui::test]
#[ignore]
fn golden_terminal_selection_edges(cx: &mut TestAppContext) {
    golden_files::write(SET, "selection_edges", &selection_edge_rows(cx));
}

// T-F1: focus reports only with mode 1004; the size claims of focus; the cursor of an unfocused
// view, and of a view whose context menu has the focus.
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
        json!([case, sent_text(), sent::claims(), shape, focused, exact])
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
const ANSWER_CASES: [(&str, &str); 13] = [
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
];

fn answer_rows(cx: &mut TestAppContext) -> Vec<Value> {
    let (_, terminal, cx) = open(cx, PANE, false);
    let mut rows = Vec::new();
    for (case, query) in ANSWER_CASES {
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

// "Clear" of the menu: the history and the screen go, but the line of the cursor stays: it
// moves to the top row and the cursor keeps its column. The shell is not told.
#[gpui::test]
fn clear_keeps_the_line_of_the_cursor(cx: &mut TestAppContext) {
    let (_, terminal, cx) = open(cx, PANE, false);
    feed(&terminal, &format!("{}prompt$ ab", history(40)), cx);
    zed(&terminal, cx).update(cx, |screen, _| screen.clear());
    settle(cx);
    let cursor = content(&terminal, cx, |content| content.cursor.point);
    let text = zed(&terminal, cx).read_with(cx, |screen, _| screen.get_content());
    assert_eq!(text.trim_end(), "prompt$ ab");
    #[cfg(feature = "own-terminal")]
    let cursor = (cursor.line.0, cursor.column.0);
    #[cfg(not(feature = "own-terminal"))]
    let cursor = (cursor.line, cursor.column);
    assert_eq!(cursor, (0, 10));
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
        cx.subscribe(&screen, move |_, event: &Event, _| {
            if matches!(event, Event::Wakeup) {
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

/// The rows of the matrix the own model answers alone (S2.3): T-Q1, T-F1, T-R1 and T-E1, on the
/// same goldens and facts as the old rows above. What the view adds (the size claims of focus,
/// focus into its menu counting as no focus out, the grid of a pane) waits for the own view
/// (S2.4).
#[cfg(feature = "own-terminal")]
mod own {
    use std::cell::Cell;
    use std::rc::Rc;
    use std::time::Duration;

    use gpui::{TestAppContext, px};
    use serde_json::{Value, json};

    use super::{ANSWER_CASES, SET, history};
    use crate::app::golden_files;
    use crate::terminals::term::model::TermEvent;
    use crate::terminals::term::model::tests::{Screen, bounds, feed, screen, sent, settle};

    // T-Q1: the answers go out through the shell's input in the order of the queries.
    #[gpui::test]
    fn the_own_answers_match_the_golden(cx: &mut TestAppContext) {
        let mut own = screen(cx, (79, 20));
        let mut rows = Vec::new();
        for (case, query) in ANSWER_CASES {
            feed(&mut own, query, cx);
            rows.push(json!([case, sent(&mut own)]));
        }
        golden_files::check(SET, "answers", &rows);
    }

    fn focus(own: &Screen, focused: bool, cx: &mut TestAppContext) {
        own.model.update(cx, |model, _| {
            if focused {
                model.focus_in()
            } else {
                model.focus_out()
            }
        });
        settle(&own.model, cx);
    }

    // T-F1: focus reports only with mode 1004, and the cursor of an unfocused screen. The view
    // tells the model only of focus on the view itself: the menu taking the focus and giving it
    // back is no change, as the old rows show.
    #[gpui::test]
    fn the_own_focus_reports_match_the_golden(cx: &mut TestAppContext) {
        let mut own = screen(cx, (79, 20));
        focus(&own, true, cx);
        feed(&mut own, "prompt$ ", cx);
        sent(&mut own);
        // The case, the program's output, then the focus the view tells.
        let steps = [
            ("focused", "", None),
            ("blurred, no mode", "", Some(false)),
            ("focused, no mode", "", Some(true)),
            ("mode 1004 on", "\x1b[?1004h", None),
            ("blurred, mode 1004", "", Some(false)),
            ("focused, mode 1004", "", Some(true)),
            ("context menu open", "", None),
            ("focused after the menu", "", None),
            ("blurred, mode 1004 off", "\x1b[?1004l", Some(false)),
        ];
        let mut rows = Vec::new();
        for (case, output, focused) in steps {
            if !output.is_empty() {
                feed(&mut own, output, cx);
            }
            if let Some(focused) = focused {
                focus(&own, focused, cx);
            }
            let shape = own.model.read_with(cx, |model, _| {
                format!("{:?}", model.last_content().cursor.shape)
            });
            rows.push(json!([case, sent(&mut own), shape]));
        }
        let golden: Vec<Value> = golden_files::read(SET, "focus")
            .iter()
            .map(|row| json!([row[0], row[1], row[3]]))
            .collect();
        assert_eq!(rows, golden);
    }

    // T-R1: a resize reflows and keeps the text; the history limit stays; a change of a few
    // pixels is no new grid.
    #[gpui::test]
    fn the_own_screen_keeps_the_text_and_the_history_limit_on_a_resize(cx: &mut TestAppContext) {
        let mut own = screen(cx, (79, 20));
        let long: String = (0..120)
            .map(|i| char::from(b'a' + (i % 26) as u8))
            .collect();
        feed(&mut own, &format!("{long}\r\nshort\r\n"), cx);
        let history = own.model.read_with(cx, |model, _| model.history());
        let resize = |own: &Screen, columns: usize, cx: &mut TestAppContext| {
            own.model
                .update(cx, |model, _| model.set_size(bounds(columns, 20)));
            settle(&own.model, cx);
            own.model.read_with(cx, |model, _| {
                (model.last_content().columns, model.get_content())
            })
        };
        let (columns, text) = resize(&own, 40, cx);
        assert_eq!(columns, 40, "the grid did not shrink");
        assert!(
            text.replace('\n', "").contains(&long),
            "the long line was lost: {text}"
        );
        let (columns, text) = resize(&own, 79, cx);
        assert_eq!(columns, 79);
        assert!(text.contains(&long[..79]), "{text}");
        assert_eq!(own.model.read_with(cx, |model, _| model.history()), history);

        // Less than a cell: the grid stays and nothing reaches the shell.
        let grid = |own: &Screen, cx: &mut TestAppContext| {
            own.model.read_with(cx, |model, _| {
                let content = model.last_content();
                (content.columns, content.screen_lines)
            })
        };
        let before = grid(&own, cx);
        let mut wider = own
            .model
            .read_with(cx, |model, _| model.last_content().terminal_bounds);
        wider.bounds.size.width += px(3.);
        own.model.update(cx, |model, _| model.set_size(wider));
        settle(&own.model, cx);
        assert_eq!(grid(&own, cx), before, "a few pixels changed the grid");
        assert_eq!(sent(&mut own), "");
    }

    // T-E1: the first event after a quiet pace window wakes at once, a burst within the pace
    // wakes once after it, a steady stream once per pace window, and a writer of a replaced
    // screen wakes nothing. The pace is not the default, as in the old row.
    #[gpui::test]
    fn the_own_event_loop_wakes_once_per_pace_and_drops_a_replaced_screen(cx: &mut TestAppContext) {
        let mut own = screen(cx, (79, 20));
        let model = own.model.clone();
        let pace = Duration::from_millis(40);
        model.update(cx, |model, _| model.set_wakeup_pace(pace));
        assert_eq!(model.read_with(cx, |model, _| model.wakeup_pace()), pace);
        let wakes = Rc::new(Cell::new(0));
        let counted = wakes.clone();
        cx.update(|cx| {
            cx.subscribe(&model, move |_, event: &TermEvent, _| {
                if *event == TermEvent::Wakeup {
                    counted.set(counted.get() + 1);
                }
            })
            .detach();
        });
        let step = |ms: u64, cx: &mut TestAppContext| {
            cx.executor().advance_clock(Duration::from_millis(ms));
            cx.run_until_parked();
        };
        step(200, cx);
        wakes.set(0);

        own.writer.write(b"a");
        cx.run_until_parked();
        assert_eq!(wakes.get(), 1, "the first event after quiet waited");
        own.writer.write(b"b");
        step(5, cx);
        own.writer.write(b"c");
        step(5, cx);
        assert_eq!(wakes.get(), 1, "a burst woke before the pace ended");
        step(40, cx);
        assert_eq!(wakes.get(), 2, "a burst woke more than once");

        step(200, cx);
        wakes.set(0);
        // 20 writes, 10 ms apart: one at once, then one per 40 ms window up to 200 ms.
        for _ in 0..20 {
            own.writer.write(b"x");
            step(10, cx);
        }
        step(200, cx);
        assert_eq!(wakes.get(), 6, "a steady stream woke {} times", wakes.get());

        model
            .update(cx, |model, cx| model.replace_display_output(&[], cx))
            .unwrap();
        step(200, cx);
        wakes.set(0);
        own.writer.write(b"old");
        step(100, cx);
        assert_eq!(
            wakes.get(),
            0,
            "a replaced screen's writer woke the new screen"
        );
        own.renew_writer(cx);
        own.writer.write(b"new");
        step(100, cx);
        assert_eq!(wakes.get(), 1);
    }

    // T-I2: text the input method composes sends nothing and shows at the cursor; a commit sends
    // it once; an unmark drops it. The handler offers no text: its selection is empty at 0, and
    // the rect of a range is the cursor's cell moved right a cell per character before it.
    #[gpui::test]
    fn the_input_method_composes_at_the_cursor_and_sends_only_a_commit(cx: &mut TestAppContext) {
        use gpui::InputHandler as _;

        use crate::terminals::term::ime::TermInput;

        let (_, terminal, cx) = super::open(cx, super::PANE, false);
        super::feed(&terminal, "prompt$ ", cx);
        let view = terminal.read_with(cx, |terminal, _| terminal.view.clone());
        let (cursor, cell_width) = super::content(&terminal, cx, |content| {
            let grid = content.terminal_bounds;
            let at = gpui::point(
                grid.bounds.origin.x
                    + (grid.cell_width * content.cursor.point.column.0 as f32).floor(),
                grid.bounds.origin.y
                    + (grid.line_height * content.cursor.point.line.0 as f32).floor(),
            );
            (at, grid.cell_width)
        });
        let mut input = TermInput::new(view.clone());
        cx.update(|window, cx| {
            input.replace_and_mark_text_in_range(None, "vie\u{302}", None, window, cx);
            assert_eq!(input.marked_text_range(window, cx), Some(0..4));
            let selected = input.selected_text_range(false, window, cx).unwrap();
            assert_eq!(selected.range, 0..0);
            assert_eq!(input.text_for_range(0..1, &mut None, window, cx), None);
            let first = input.bounds_for_range(0..0, window, cx).unwrap();
            assert_eq!(first.origin, cursor);
            let third = input.bounds_for_range(2..3, window, cx).unwrap();
            assert_eq!(third.origin.x, cursor.x + cell_width * 2.);
            assert_eq!(third.size, first.size);
        });
        super::settle(cx);
        assert_eq!(super::sent_text(), "", "marked text reached the shell");
        cx.update(|window, cx| {
            input.unmark_text(window, cx);
            assert_eq!(input.marked_text_range(window, cx), None);
            input.replace_and_mark_text_in_range(None, "vi", None, window, cx);
            input.replace_text_in_range(None, "việt", window, cx);
            assert_eq!(input.marked_text_range(window, cx), None);
        });
        super::settle(cx);
        assert_eq!(super::sent::take(), ["việt"]);
    }

    // The scrollbar is drawn only in the states that show it; its thumb sits at the bottom of
    // the track when the screen shows the newest lines, and goes up as the screen scrolls up.
    #[gpui::test]
    fn the_thumb_follows_the_scroll(cx: &mut TestAppContext) {
        let (_, terminal, cx) = super::open(cx, super::PANE, false);
        super::feed(&terminal, &super::history(200), cx);
        let view = terminal.read_with(cx, |terminal, _| terminal.view.clone());
        let thumb = |cx: &mut gpui::VisualTestContext| {
            super::content(&terminal, cx, |_| ());
            view.read_with(cx, |view, _| view.laid.and_then(|laid| laid.thumb))
        };
        let bottom = thumb(cx).expect("no thumb over 200 lines of history");
        terminal.update(cx, |terminal, cx| {
            terminal
                .terminal()
                .update(cx, |model, _| model.scroll_up_by(100))
        });
        super::settle(cx);
        let up = thumb(cx).unwrap();
        assert!(up.origin.y < bottom.origin.y, "{up:?} {bottom:?}");
        assert_eq!(up.size, bottom.size);
    }

    // A show of a scrollbar that shows already keeps it as it is and only starts its hide delay
    // again: a quick second scroll, and output that adds lines on each frame, keep it at 1. A
    // thumb the pointer leaves or lets go of stays a moment, then fades.
    #[gpui::test]
    fn the_scrollbar_keeps_showing_while_it_is_shown_again(cx: &mut TestAppContext) {
        use gpui::{Modifiers, MouseButton, VisualTestContext, point};

        fn wait(ms: u64, cx: &mut VisualTestContext) {
            cx.executor().advance_clock(Duration::from_millis(ms));
            cx.run_until_parked();
        }

        let (_, terminal, cx) = super::open(cx, super::PANE, false);
        super::feed(&terminal, &history(200), cx);
        let view = terminal.read_with(cx, |terminal, _| terminal.view.clone());
        let alpha = |cx: &mut VisualTestContext| view.read_with(cx, |view, cx| view.bar_alpha(cx));
        let hide = |cx: &mut VisualTestContext| {
            wait(2000, cx);
            super::draw(cx);
            assert_eq!(alpha(cx), 0.);
        };

        hide(cx);
        view.update(cx, |view, cx| view.show_bar(cx));
        wait(50, cx);
        assert_eq!(alpha(cx), 1.);
        view.update(cx, |view, cx| view.show_bar(cx));
        wait(10, cx);
        assert_eq!(
            alpha(cx),
            1.,
            "a second show 10 ms later lowered the scrollbar"
        );

        hide(cx);
        let screen = super::zed(&terminal, cx);
        let mut total = super::content(&terminal, cx, |content| content.total_lines);
        for frame in 0..8 {
            screen.update(cx, |screen, cx| screen.write_output(b"more\r\n", cx));
            let now = super::content(&terminal, cx, |content| content.total_lines);
            assert!(now > total, "frame {frame} added no line");
            total = now;
            wait(16, cx);
            if frame >= 3 {
                assert_eq!(alpha(cx), 1., "frame {frame} of steady output");
            }
        }

        let thumb_and_away = |cx: &mut VisualTestContext| {
            super::content(&terminal, cx, |_| ());
            let laid = view.read_with(cx, |view, _| view.laid).unwrap();
            let thumb = laid.thumb.expect("no thumb over the history").center();
            (thumb, point(laid.bounds.center().x, thumb.y))
        };
        let none = Modifiers::none();

        // The pointer on a hidden thumb shows it; leaving it keeps it for a moment.
        hide(cx);
        let (thumb, away) = thumb_and_away(cx);
        super::mouse_move(thumb, None, none, cx);
        assert_eq!(alpha(cx), 1.);
        wait(2000, cx);
        super::mouse_move(away, None, none, cx);
        assert_eq!(alpha(cx), 1., "the thumb hid as the pointer left it");
        wait(900, cx);
        assert_eq!(alpha(cx), 1.);
        wait(600, cx);
        assert_eq!(alpha(cx), 0.);

        // A thumb let go of away from it after a long drag stays for a moment too.
        hide(cx);
        let (thumb, away) = thumb_and_away(cx);
        super::mouse_move(thumb, None, none, cx);
        super::mouse_down(thumb, MouseButton::Left, none, 1, cx);
        super::mouse_move(away, Some(MouseButton::Left), none, cx);
        wait(2000, cx);
        assert_eq!(alpha(cx), 1.);
        super::mouse_up(away, MouseButton::Left, none, 1, cx);
        assert_eq!(alpha(cx), 1., "the thumb hid as it was let go of");
        wait(1500, cx);
        assert_eq!(alpha(cx), 0.);
    }

    // T-E1: a scroll, a resize and a select queued before one sync run in that order, and give
    // what the old rows pinned: Select All, asked before the resize, leaves out the 9 lines the
    // resize took from the screen.
    #[gpui::test]
    fn the_own_queued_changes_run_in_their_order_within_one_sync(cx: &mut TestAppContext) {
        let mut own = screen(cx, (79, 20));
        let model = own.model.clone();
        let state = |cx: &mut TestAppContext| {
            model.read_with(cx, |model, _| {
                let content = model.last_content();
                json!([
                    content.display_offset,
                    content.columns,
                    content.screen_lines,
                    content.selection_text
                ])
            })
        };

        // One sync each.
        feed(&mut own, &history(120), cx);
        model.update(cx, |model, _| model.scroll_up_by(5));
        settle(&model, cx);
        model.update(cx, |model, _| model.set_size(bounds(48, 11)));
        settle(&model, cx);
        model.update(cx, |model, _| model.select_all());
        settle(&model, cx);
        let apart = state(cx);

        // The same on a new screen, all before one sync.
        model.update(cx, |model, _| model.set_size(bounds(79, 20)));
        settle(&model, cx);
        model
            .update(cx, |model, cx| model.replace_display_output(&[], cx))
            .unwrap();
        own.renew_writer(cx);
        feed(&mut own, &history(120), cx);
        model.update(cx, |model, _| {
            model.scroll_up_by(5);
            model.set_size(bounds(48, 11));
            model.select_all();
        });
        settle(&model, cx);
        let together = state(cx);
        let lines =
            |from: usize| -> String { (from..=120).map(|n| format!("line {n}\n")).collect() };
        assert_eq!(apart, json!([14, 48, 11, lines(1)]));
        assert_eq!(together, json!([14, 48, 11, lines(10)]));
    }
}
