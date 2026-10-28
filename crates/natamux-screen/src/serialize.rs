//! A snapshot that xterm.js replays to the same screen, as xterm's `SerializeAddon` writes it:
//! the normal buffer with its history, the alternate screen when it is on, the cursor and the
//! modes. The text and colours are the same; the bytes may differ.

use std::fmt::Write;

use alacritty_terminal::event::VoidListener;
use alacritty_terminal::grid::{Dimensions, Grid};
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::{Cell, Flags};
use alacritty_terminal::term::{Term, TermMode};
use alacritty_terminal::vte::ansi::{Color, NamedColor};

/// `max_history` keeps only the newest rows of the history above the screen; `None` keeps all.
pub fn serialize(term: &mut Term<VoidListener>, max_history: Option<usize>) -> Vec<u8> {
    let mut out = String::new();
    let mode = *term.mode();
    if mode.contains(TermMode::ALT_SCREEN) {
        // alacritty keeps the normal buffer private while the alternate screen is on. Switching
        // back and forth shows it; the alternate screen is copied before, as the switch clears it.
        let alternate = term.grid().clone();
        term.swap_alt();
        write_grid(&mut out, term.grid(), max_history);
        term.swap_alt();
        *term.grid_mut() = alternate;
        out.push_str("\x1b[?1049h\x1b[H");
    }
    write_grid(&mut out, term.grid(), max_history);
    write_modes(&mut out, mode);
    // The pen a program set last, for the output that comes after the snapshot.
    let pen = Style::of(&term.grid().cursor.template);
    if pen != Style::default() {
        write_sgr(&mut out, &pen);
    }
    out.into_bytes()
}

/// Rows from the top of the history down, then the cursor. With no history, as `SerializeAddon`,
/// the rows end at the last one with content or the cursor; with history every row is written,
/// so the screen starts at the same row. History lines have negative numbers, so the newest
/// `max_history` rows start at `-max_history`. A cut that lands inside a wrapped line starts at
/// its tail: that row is written as it is, and nothing after it depends on the rows before.
fn write_grid(out: &mut String, grid: &Grid<Cell>, max_history: Option<usize>) {
    let top = match max_history {
        Some(max) => grid
            .topmost_line()
            .0
            .max(-i32::try_from(max).unwrap_or(i32::MAX)),
        None => grid.topmost_line().0,
    };
    let columns = grid.columns();
    let cursor = grid.cursor.point;
    let last = if grid.history_size() > 0 {
        grid.bottommost_line().0
    } else {
        let content = (0..=grid.bottommost_line().0)
            .rev()
            .find(|row| row_end(grid, *row) > 0)
            .unwrap_or(0);
        content.max(cursor.line.0)
    };
    let mut pen = Style::default();
    let mut pending_wrap = false;
    let mut at = 0;
    for row in top..=last {
        let cells = &grid[Line(row)];
        let wrapped = cells[Column(columns - 1)].flags.contains(Flags::WRAPLINE);
        let end = row_end(grid, row);
        at = end;
        if pending_wrap && end == 0 {
            // The rest of a wrapped line that is empty: one blank makes the terminal wrap.
            out.push(' ');
            at = 1;
        }
        for column in 0..end {
            let cell = &cells[Column(column)];
            if cell
                .flags
                .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
            {
                continue;
            }
            let style = Style::of(cell);
            if style != pen {
                write_sgr(out, &style);
                pen = style;
            }
            out.push(if cell.c == '\0' { ' ' } else { cell.c });
            out.extend(cell.zerowidth().unwrap_or_default());
        }
        pending_wrap = wrapped;
        if row < last && !wrapped {
            // A new row at the bottom takes the background of the pen.
            if pen != Style::default() {
                out.push_str("\x1b[0m");
                pen = Style::default();
            }
            out.push_str("\r\n");
        }
    }
    if pen != Style::default() {
        out.push_str("\x1b[0m");
    }
    // Where the writing left the cursor, as a terminal that just wrote it holds it.
    let column = if grid.cursor.input_needs_wrap {
        columns
    } else {
        cursor.column.0
    };
    if (cursor.line.0, column) != (last, at) {
        let _ = write!(out, "\x1b[{};{}H", cursor.line.0 + 1, cursor.column.0 + 1);
    }
}

/// How much of a row is written: a wrapped row whole, so the terminal wraps at the same place;
/// any other up to its last cell that shows something.
fn row_end(grid: &Grid<Cell>, row: i32) -> usize {
    let columns = grid.columns();
    if grid[Line(row)][Column(columns - 1)]
        .flags
        .contains(Flags::WRAPLINE)
    {
        columns
    } else {
        content_end(grid, row)
    }
}

/// The columns up to the last cell that shows something: a character, a background or an
/// attribute.
fn content_end(grid: &Grid<Cell>, row: i32) -> usize {
    let cells = &grid[Line(row)];
    let blank = |cell: &Cell| {
        (cell.c == ' ' || cell.c == '\0')
            && cell.bg == Color::Named(NamedColor::Background)
            && (cell.flags - Flags::WRAPLINE).is_empty()
            && cell.zerowidth().is_none()
    };
    (0..grid.columns())
        .rev()
        .find(|column| !blank(&cells[Column(*column)]))
        .map_or(0, |column| column + 1)
}

/// The modes `SerializeAddon._serializeModes` writes, in its order, and the ones it cannot read
/// back from xterm but the dashboard needs: the mouse encoding and a hidden cursor.
fn write_modes(out: &mut String, mode: TermMode) {
    let on = [
        (TermMode::APP_CURSOR, "\x1b[?1h"),
        (TermMode::APP_KEYPAD, "\x1b[?66h"),
        (TermMode::BRACKETED_PASTE, "\x1b[?2004h"),
        (TermMode::INSERT, "\x1b[4h"),
        (TermMode::ORIGIN, "\x1b[?6h"),
        (TermMode::FOCUS_IN_OUT, "\x1b[?1004h"),
    ];
    for (flag, sequence) in on {
        if mode.contains(flag) {
            out.push_str(sequence);
        }
    }
    if !mode.contains(TermMode::LINE_WRAP) {
        out.push_str("\x1b[?7l");
    }
    if mode.contains(TermMode::MOUSE_MOTION) {
        out.push_str("\x1b[?1003h");
    } else if mode.contains(TermMode::MOUSE_DRAG) {
        out.push_str("\x1b[?1002h");
    } else if mode.contains(TermMode::MOUSE_REPORT_CLICK) {
        out.push_str("\x1b[?1000h");
    }
    if mode.contains(TermMode::SGR_MOUSE) {
        out.push_str("\x1b[?1006h");
    }
    // The dashboard resets before a snapshot, which shows the cursor. A program that hid it to
    // draw its own would then have two.
    if !mode.contains(TermMode::SHOW_CURSOR) {
        out.push_str("\x1b[?25l");
    }
}

/// What SGR sets for a cell.
#[derive(Clone, Copy, PartialEq)]
struct Style {
    fg: Color,
    bg: Color,
    flags: Flags,
    underline: Option<Color>,
}

impl Default for Style {
    fn default() -> Style {
        Style {
            fg: Color::Named(NamedColor::Foreground),
            bg: Color::Named(NamedColor::Background),
            flags: Flags::empty(),
            underline: None,
        }
    }
}

impl Style {
    fn of(cell: &Cell) -> Style {
        let shown = Flags::BOLD
            | Flags::DIM
            | Flags::ITALIC
            | Flags::ALL_UNDERLINES
            | Flags::INVERSE
            | Flags::HIDDEN
            | Flags::STRIKEOUT;
        Style {
            fg: cell.fg,
            bg: cell.bg,
            flags: cell.flags & shown,
            underline: cell.underline_color(),
        }
    }
}

/// Writes the SGR sequence that sets `style` from a reset pen. Into the snapshot itself: a
/// snapshot of a coloured screen has one per change of style.
fn write_sgr(out: &mut String, style: &Style) {
    out.push_str("\x1b[0");
    let flags = [
        (Flags::BOLD, "1"),
        (Flags::DIM, "2"),
        (Flags::ITALIC, "3"),
        (Flags::UNDERLINE, "4"),
        (Flags::DOUBLE_UNDERLINE, "4:2"),
        (Flags::UNDERCURL, "4:3"),
        (Flags::DOTTED_UNDERLINE, "4:4"),
        (Flags::DASHED_UNDERLINE, "4:5"),
        (Flags::INVERSE, "7"),
        (Flags::HIDDEN, "8"),
        (Flags::STRIKEOUT, "9"),
    ];
    for (flag, code) in flags {
        if style.flags.contains(flag) {
            out.push(';');
            out.push_str(code);
        }
    }
    write_color(out, style.fg, 30, 90, 38);
    write_color(out, style.bg, 40, 100, 48);
    if let Some(underline) = style.underline {
        write_color(out, underline, 0, 0, 58);
    }
    out.push('m');
}

/// The SGR codes of a colour: 16 colours as `base + n` and `bright + n` (xterm keeps them apart
/// from the same 256-colour index), the 256 colours as `extended;5;n`, true colour as
/// `extended;2;r;g;b`. A default colour adds nothing.
fn write_color(out: &mut String, color: Color, base: u16, bright: u16, extended: u16) {
    let _ = match color {
        Color::Named(named) => {
            let index = named as u16;
            match index {
                0..=7 if base > 0 => write!(out, ";{}", base + index),
                8..=15 if bright > 0 => write!(out, ";{}", bright + index - 8),
                0..=15 => write!(out, ";{extended};5;{index}"),
                _ => Ok(()),
            }
        }
        Color::Indexed(index) => write!(out, ";{extended};5;{index}"),
        Color::Spec(rgb) => write!(out, ";{extended};2;{};{};{}", rgb.r, rgb.g, rgb.b),
    };
}

#[cfg(test)]
mod tests {
    use crate::Screen;

    /// A snapshot replayed into a fresh screen of the same size gives the same snapshot.
    fn round_trip(cols: u16, rows: u16, output: &[u8]) -> (Vec<u8>, Screen) {
        let mut screen = Screen::new(cols, rows);
        screen.write(output);
        let first = screen.snapshot();
        let mut copy = Screen::new(cols, rows);
        copy.write(&first);
        let second = copy.snapshot();
        assert_eq!(
            String::from_utf8_lossy(&second),
            String::from_utf8_lossy(&first)
        );
        (first, copy)
    }

    fn text(bytes: &[u8]) -> String {
        String::from_utf8_lossy(bytes).into_owned()
    }

    #[test]
    fn keeps_colours_attributes_and_wide_text() {
        let output = "\x1b[1;31mred\x1b[0m \x1b[38;5;1mindex\x1b[0m \x1b[48;2;1;2;3mrgb\x1b[0m\r\n\
            \x1b[3;4;9mstyle\x1b[0m người 中文 😀\r\n\x1b[7;2mdim\x1b[0m";
        let (snapshot, copy) = round_trip(40, 5, output.as_bytes());
        let snapshot = text(&snapshot);
        assert!(snapshot.contains("\x1b[0;1;31mred"), "{snapshot:?}");
        assert!(snapshot.contains("\x1b[0;38;5;1mindex"), "{snapshot:?}");
        assert!(snapshot.contains("\x1b[0;48;2;1;2;3mrgb"), "{snapshot:?}");
        assert!(copy.last_lines(5).contains("người 中文 😀"));
    }

    #[test]
    fn writes_bright_colours_and_the_underline_colour() {
        let (snapshot, _) = round_trip(20, 4, b"\x1b[92;4;58;5;3mlink\x1b[0m");
        let snapshot = text(&snapshot);
        assert!(snapshot.contains("\x1b[0;4;92;58;5;3mlink"), "{snapshot:?}");
    }

    #[test]
    fn joins_wrapped_lines_and_keeps_the_history() {
        let mut output = String::new();
        for line in 0..30 {
            output.push_str(&format!("line {line}\r\n"));
        }
        output.push_str(&"x".repeat(25));
        let (snapshot, copy) = round_trip(10, 4, output.as_bytes());
        assert!(text(&snapshot).contains("xxxxxxxxxxxxxxxxxxxxxxxxx"));
        assert!(copy.last_lines(40).starts_with("line 0\nline 1\n"));
        assert!(copy.last_lines(1).ends_with("xxxxx"));
    }

    #[test]
    fn a_wrapped_line_whose_rest_is_empty_still_wraps() {
        let (_, copy) = round_trip(5, 3, b"abcde");
        assert_eq!(copy.last_lines(3), "abcde");
    }

    #[test]
    fn keeps_the_alternate_screen_above_the_normal_one() {
        let (snapshot, mut copy) = round_trip(20, 4, b"shell$ \x1b[?1049h\x1b[Hvim screen");
        assert!(text(&snapshot).contains("shell$"));
        assert!(text(&snapshot).contains("\x1b[?1049h\x1b[H"));
        assert_eq!(copy.last_lines(4), "vim screen");
        copy.write(b"\x1b[?1049l");
        assert_eq!(copy.last_lines(4), "shell$");
    }

    #[test]
    fn writes_the_cursor_and_the_modes() {
        let output =
            b"ab\x1b[2;5H\x1b[?1h\x1b=\x1b[?2004h\x1b[?1002h\x1b[?1006h\x1b[?1004h\x1b[?25l";
        // Not a round trip: alacritty does not read `?66h`, the keypad mode as xterm.js writes it.
        let mut screen = Screen::new(20, 4);
        screen.write(output);
        let snapshot = text(&screen.snapshot());
        for part in [
            "\x1b[2;5H",
            "\x1b[?1h",
            "\x1b[?66h",
            "\x1b[?2004h",
            "\x1b[?1002h",
            "\x1b[?1006h",
            "\x1b[?1004h",
            "\x1b[?25l",
        ] {
            assert!(snapshot.contains(part), "{part:?} in {snapshot:?}");
        }
        let (plain, _) = round_trip(20, 4, b"ab");
        assert!(!text(&plain).contains("?25l"));
    }

    #[test]
    fn an_empty_screen_is_an_empty_snapshot_and_rows_end_at_the_cursor() {
        assert!(Screen::new(20, 4).snapshot().is_empty());
        let (snapshot, _) = round_trip(20, 4, b"ab\r\n\r\n");
        assert_eq!(text(&snapshot), "ab\r\n\r\n");
        let (snapshot, _) = round_trip(20, 4, b"ab\x1b[1;2H");
        assert_eq!(text(&snapshot), "ab\x1b[1;2H");
    }

    #[test]
    fn restores_the_pen_a_program_set_last() {
        let (snapshot, _) = round_trip(20, 4, b"\x1b[32mgreen");
        assert!(text(&snapshot).ends_with("\x1b[0;32m"));
    }

    fn numbered(count: usize) -> String {
        (0..count).map(|line| format!("line {line}\r\n")).collect()
    }

    #[test]
    fn a_cap_keeps_the_last_history_rows_and_the_screen() {
        let mut screen = Screen::new(10, 4);
        screen.write(numbered(30).as_bytes());
        // 27 history rows (0..=26), then 27, 28, 29 and the cursor row on the screen.
        let tail = text(&screen.snapshot_tail(5));
        assert!(tail.starts_with("line 22\r\nline 23\r\n"), "{tail:?}");
        assert!(!tail.contains("line 21"), "{tail:?}");
        let mut copy = Screen::new(10, 4);
        copy.write(tail.as_bytes());
        let shown = copy.last_lines(usize::MAX);
        assert_eq!(shown.lines().count(), 8, "{shown:?}");
        assert!(shown.ends_with("line 29"), "{shown:?}");
    }

    #[test]
    fn a_cap_larger_than_the_history_or_no_cap_gives_the_same_snapshot() {
        let mut screen = Screen::new(10, 4);
        screen.write(numbered(30).as_bytes());
        let full = screen.snapshot();
        assert_eq!(screen.snapshot_tail(27), full);
        assert_eq!(screen.snapshot_tail(usize::MAX), full);
    }

    #[test]
    fn a_cut_inside_a_wrapped_line_starts_at_its_tail() {
        let mut screen = Screen::new(10, 4);
        let output = format!("{}\r\n{}", "y".repeat(25), numbered(5));
        screen.write(output.as_bytes());
        // History: the 3 rows of the wrapped line, line 0, line 1. The cap starts at its 2nd row.
        let tail = text(&screen.snapshot_tail(4));
        assert!(
            tail.starts_with("yyyyyyyyyyyyyyy\r\nline 0\r\nline 1\r\n"),
            "{tail:?}"
        );
    }

    #[test]
    fn a_cap_keeps_the_alternate_screen_and_the_modes() {
        let mut screen = Screen::new(20, 4);
        screen.write(numbered(12).as_bytes());
        screen.write(b"\x1b[?1049h\x1b[Hvim screen\x1b[?2004h");
        let tail = text(&screen.snapshot_tail(2));
        assert!(tail.contains("\x1b[?1049h\x1b[H"), "{tail:?}");
        assert!(tail.contains("vim screen"), "{tail:?}");
        assert!(tail.contains("\x1b[?2004h"), "{tail:?}");
        assert!(!tail.contains("line 0\r\n"), "{tail:?}");
        assert!(tail.contains("line 11"), "{tail:?}");
    }
}
