//! The terminal `--shot` states of the MIT move (docs/plan-mit-license.md 7.2): every kind of
//! cell, the cursor shapes, a selection over find matches, the alternate screen, history scrolled
//! up, the context menu and the scrollbar. Each one gives a terminal a screen of its own in the
//! fake; the steps then act as a user would: a focus, a drag, a wheel turn, a right click.

use std::fmt::Write as _;
use std::time::Duration;

use gpui::{App, AsyncApp, Focusable, MouseButton, Pixels, Point, WindowHandle, point};

use super::fake_daemon;
use super::options::Shot;
use super::shots::{self, ASKING, PANES_DRAWN, REVIEWED, RUNNING};
use super::shots_input::{click, drag, send, wait, wheel};
use super::window::Root;
use crate::terminals::terminal_cache::{TerminalCache, terminal_key};

/// The terminal of the asking task: one pane, the most room.
const ONE: &str = "1";
/// The side by side group of the running task, and its plain terminal.
const GROUP: [(&str, &str); 4] = [
    ("1", "block"),
    ("10", "underline"),
    ("12", "bar"),
    ("2", "block, never focused"),
];
/// The find runs on a background task; its matches are in well within this.
const FOUND: Duration = Duration::from_millis(600);
/// The focus is in place for the drag within this.
const FOCUSED: Duration = Duration::from_millis(100);
/// History lines of the scrolled and scrollbar states, and the wheel turn up into them.
const HISTORY_LINES: usize = 300;
const SCROLLED_LINES: f32 = 12.;
/// The cells (line, column) of the drag of the selection: from `Read` of line 3 into line 4.
const DRAG: ((usize, usize), (usize, usize)) = ((2, 2), (3, 20));
/// The cell of the right click: inside `Claude` on the first line of the asking agent.
const RIGHT_CLICK: (usize, usize) = (0, 4);

pub fn is_terminal_shot(shot: Shot) -> bool {
    matches!(
        shot,
        Shot::TerminalLook
            | Shot::TerminalCursors
            | Shot::TerminalCursorsUnderline
            | Shot::TerminalCursorsBar
            | Shot::TerminalSelection
            | Shot::TerminalAltScreen
            | Shot::TerminalScrolled
            | Shot::TerminalContextMenu
            | Shot::TerminalScrollbar
    )
}

/// The screens of the state, before the window opens.
pub fn prepare(shot: Shot) {
    let fake = fake_daemon::shared();
    match shot {
        Shot::TerminalLook => fake.set_screen(ASKING, ONE, look()),
        Shot::TerminalAltScreen => fake.set_screen(ASKING, ONE, alt_screen()),
        Shot::TerminalScrolled | Shot::TerminalScrollbar => fake.set_screen(ASKING, ONE, history()),
        Shot::TerminalCursors | Shot::TerminalCursorsUnderline | Shot::TerminalCursorsBar => {
            for (terminal, shape) in GROUP {
                fake.set_screen(RUNNING, terminal, cursor_screen(shape));
            }
        }
        _ => {}
    }
}

pub async fn run(handle: WindowHandle<Root>, shot: Shot, cx: &mut AsyncApp) {
    let task = task_of(shot);
    let opened = handle.update(cx, |root, _, cx| shots::open(root.dashboard(), task, cx));
    if opened.is_err() {
        return;
    }
    wait(PANES_DRAWN, cx).await;
    if task == RUNNING {
        shape_cursors(handle, cx);
    }
    match shot {
        Shot::TerminalCursors => focus(handle, RUNNING, "1", cx),
        Shot::TerminalCursorsUnderline => focus(handle, RUNNING, "10", cx),
        Shot::TerminalCursorsBar => focus(handle, RUNNING, "12", cx),
        Shot::TerminalSelection => {
            handle
                .update(cx, |_, window, cx| {
                    shots::find_in_pane(REVIEWED, window, cx)
                })
                .ok();
            wait(FOUND, cx).await;
            focus(handle, REVIEWED, ONE, cx);
            wait(FOCUSED, cx).await;
            let (from, to) = DRAG;
            let from = cell(handle, REVIEWED, from, cx);
            let to = cell(handle, REVIEWED, to, cx);
            if let (Some(from), Some(to)) = (from, to) {
                send(handle, drag(from, to), cx);
            }
        }
        Shot::TerminalScrolled => {
            if let Some(at) = cell(handle, ASKING, (2, 10), cx) {
                send(handle, wheel(at, SCROLLED_LINES), cx);
            }
        }
        Shot::TerminalContextMenu => {
            if let Some(at) = cell(handle, ASKING, RIGHT_CLICK, cx) {
                send(handle, click(MouseButton::Right, at), cx);
            }
        }
        _ => {}
    }
}

fn task_of(shot: Shot) -> &'static str {
    match shot {
        Shot::TerminalCursors | Shot::TerminalCursorsUnderline | Shot::TerminalCursorsBar => {
            RUNNING
        }
        Shot::TerminalSelection => REVIEWED,
        _ => ASKING,
    }
}

fn focus(handle: WindowHandle<Root>, task_id: &str, terminal: &str, cx: &mut AsyncApp) {
    let key = terminal_key(task_id, terminal);
    let focused = handle.update(cx, |_, window, cx| {
        let Some(remote) = TerminalCache::cached(&key, cx) else {
            eprintln!("shot: Terminal {terminal} of {task_id} is not on screen");
            return;
        };
        let focus = remote.read(cx).view.focus_handle(cx);
        window.focus(&focus, cx);
    });
    if let Err(error) = focused {
        eprintln!("shot: the window is gone: {error}");
    }
}

/// The middle of a cell (line, column) of the terminal's screen, in the window, from where the
/// terminal laid out its grid at the last frame.
fn cell(
    handle: WindowHandle<Root>,
    task_id: &str,
    (line, column): (usize, usize),
    cx: &mut AsyncApp,
) -> Option<Point<Pixels>> {
    let key = terminal_key(task_id, ONE);
    let at = handle.update(cx, |_, _, cx| grid_point(&key, line, column, cx));
    let at = at.ok().flatten();
    if at.is_none() {
        eprintln!("shot: Terminal 1 of {task_id} has no grid yet");
    }
    at
}

fn grid_point(key: &str, line: usize, column: usize, cx: &App) -> Option<Point<Pixels>> {
    let remote = TerminalCache::cached(key, cx)?;
    let remote = remote.read(cx);
    let content = remote.terminal().read(cx).last_content();
    let grid = content.terminal_bounds;
    let origin = grid.bounds.origin;
    Some(point(
        origin.x + grid.cell_width * (column as f32 + 0.5),
        origin.y + grid.line_height * (line as f32 + 0.5),
    ))
}

/// Every kind of cell the terminal draws: the 16 colours as text and as background, a sample of
/// the 256, a truecolour row, the attributes, wide and joined characters, the drawn blocks and
/// lines, Powerline, pairs of low contrast and an OSC 8 link.
fn look() -> String {
    let mut out = String::new();
    let mut row = |title: &str, cells: String| {
        let _ = write!(out, "\x1b[2m{title:<8}\x1b[0m {cells}\x1b[0m\r\n");
    };
    let cells = |items: &mut dyn Iterator<Item = u32>, cell: fn(u32) -> String| {
        let mut text = String::new();
        for item in items {
            text.push_str(&cell(item));
        }
        text
    };
    row("fg", cells(&mut (30..=37), |c| format!("\x1b[{c}m{c} ")));
    row(
        "bright",
        cells(&mut (90..=97), |c| format!("\x1b[{c}m{c} ")),
    );
    row(
        "bg",
        cells(&mut (40..=47), |c| format!("\x1b[{c}m{c}\x1b[0m ")),
    );
    row(
        "bright",
        cells(&mut (100..=107), |c| format!("\x1b[{c}m{c}\x1b[0m ")),
    );
    row(
        "256",
        cells(&mut (16..232).step_by(9), |i| format!("\x1b[48;5;{i}m  ")),
    );
    row(
        "gray",
        cells(&mut (232..=255), |i| format!("\x1b[48;5;{i}m  ")),
    );
    row(
        "indexed",
        cells(&mut (0..16), |i| format!("\x1b[38;5;{i}m█")),
    );
    let truecolour = |i: u32| {
        let (r, g, b) = (i * 11, 255 - i * 10, 64 + i * 8);
        format!("\x1b[48;2;{r};{g};{b}m  ")
    };
    row("rgb", cells(&mut (0..24), truecolour));
    row(
        "attrs",
        "\x1b[1mbold\x1b[0m \x1b[2mdim\x1b[0m \x1b[3mitalic\x1b[0m \x1b[1;3mboth\x1b[0m \
         \x1b[9mstrike\x1b[0m \x1b[7minverse\x1b[0m \x1b[8mhidden\x1b[0m"
            .into(),
    );
    row(
        "lines",
        "\x1b[4msingle\x1b[0m \x1b[21mdouble\x1b[0m \x1b[4:3mcurly\x1b[0m \x1b[4:4mdotted\x1b[0m \
         \x1b[4;58;5;196mcoloured\x1b[0m"
            .into(),
    );
    row(
        "wide",
        "日本語 中文 한국어 \u{2764}\u{fe0f} \u{2600}\u{fe0f} \u{2714}\u{fe0f} \u{1f44d}\u{1f3fd}"
            .into(),
    );
    // Vietnamese with combining marks, not the composed letters.
    row(
        "marks",
        "Tie\u{302}\u{301}ng Vie\u{323}\u{302}t: ngu\u{31b}o\u{31b}\u{300}i, \
         đu\u{31b}o\u{31b}\u{323}c"
            .into(),
    );
    row("box", "┌─┬─┐ ╔═╦═╗ ╭─╮ ├─┼─┤ ╚═╩═╝ ╰─╯ │ ║".into());
    row("blocks", "▀▁▂▃▄▅▆▇█▉▊▋▌▍▎▏▐▔▕".into());
    row("quads", "▖▗▘▙▚▛▜▝▞▟ 🬀🬁🬂🬃🬄🬅🬆🬇🬈🬉🬊🬋".into());
    row("shades", "░░▒▒▓▓██ \x1b[34m░▒▓█\x1b[0m".into());
    row(
        "powerline",
        "\x1b[30;44m main \x1b[34;42m\u{e0b0}\x1b[30m +2 \x1b[0;32m\u{e0b0}\x1b[0m \u{e0b1} \
         \u{e0b2}\u{e0b3} \u{e0b4}\u{e0b6}"
            .into(),
    );
    row(
        "contrast",
        "\x1b[90;100m gray \x1b[0m \x1b[34;44m blue \x1b[0m \x1b[33;43m yellow \x1b[0m \
         \x1b[37;47m white \x1b[0m \x1b[30;40m black \x1b[0m"
            .into(),
    );
    row(
        "link",
        "\x1b]8;;https://example.com/natamux\x1b\\an OSC 8 link\x1b]8;;\x1b\\ and \
         https://example.com/plain"
            .into(),
    );
    out.push_str("\x1b[1m❯\x1b[0m ");
    out
}

/// A full screen program: the alternate screen in coloured rows, a title and a status line,
/// with the cursor hidden. Rows past the bottom land on the last row, so any size is full.
fn alt_screen() -> String {
    let mut out = String::from("\x1b[?1049h\x1b[?25l\x1b[H\x1b[2J");
    for row in 1..=60u32 {
        let colour = 17 + (row % 12) * 18;
        let _ = write!(
            out,
            "\x1b[{row};1H\x1b[48;5;{colour}m\x1b[K\x1b[38;5;231m row {row:02} \x1b[0m"
        );
    }
    out.push_str("\x1b[1;1H\x1b[1;30;47m\x1b[K Natamux top \x1b[0m");
    out.push_str("\x1b[999;1H\x1b[30;46m\x1b[K F1 Help  F10 Quit \x1b[0m");
    out
}

/// Numbered lines well past one screen, then a prompt.
fn history() -> String {
    let mut out = String::new();
    for line in 1..=HISTORY_LINES {
        let _ = write!(
            out,
            "{line:>3} \x1b[32mok\x1b[0m compiled crate_{} in {}.{:02}s\r\n",
            line % 17,
            line % 7,
            line % 100
        );
    }
    out.push_str("\x1b[1m❯\x1b[0m ");
    out
}

/// The text of a pane of the cursor states; `shape_cursor` then sets the shape and the place.
fn cursor_screen(shape: &str) -> String {
    format!("\x1b[1mCursor: {shape}\x1b[0m\r\n\r\n日本語 wide characters")
}

/// The cursor of `shape` (DECSCUSR 2, 4 or 6, all steady) on the wide `本` of the last line. It
/// is written once the panes laid out: a pane of the group laid out again after the fake's output
/// came, and the cursor then sat on another cell in some runs (runs 41058737599, 41069653360).
fn cursor_bytes(shape: &str) -> String {
    let code = match shape {
        "underline" => 4,
        "bar" => 6,
        _ => 2,
    };
    format!("\x1b[{code} q\r\x1b[2C")
}

fn shape_cursors(handle: WindowHandle<Root>, cx: &mut AsyncApp) {
    let written = handle.update(cx, |_, _, cx| {
        for (terminal, shape) in GROUP {
            let Some(remote) = TerminalCache::cached(&terminal_key(RUNNING, terminal), cx) else {
                eprintln!("shot: Terminal {terminal} of {RUNNING} is not on screen");
                continue;
            };
            let screen = remote.read(cx).terminal().clone();
            let bytes = cursor_bytes(shape);
            screen.update(cx, |screen, cx| screen.write_output(bytes.as_bytes(), cx));
        }
    });
    if let Err(error) = written {
        eprintln!("shot: the window is gone: {error}");
    }
}
