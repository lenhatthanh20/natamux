use std::time::Duration;

use alacritty_terminal::event::VoidListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::vte::ansi::{Processor, Timeout};
use natamux_proto::terminals::history::scrollback_lines;

use crate::serialize::serialize;

/// The screen of one shell. Output is parsed at once, in the order it came, so a snapshot holds
/// exactly the output written before it (xterm.js in the Node daemon parses later and needs a
/// queue for that).
pub struct Screen {
    term: Term<VoidListener>,
    parser: Processor<Unbuffered>,
    sync: SyncFrame,
}

impl Screen {
    pub fn new(cols: u16, rows: u16) -> Screen {
        let config = Config {
            scrolling_history: scrollback_lines(cols),
            ..Config::default()
        };
        Screen {
            term: Term::new(config, &Size::new(cols, rows), VoidListener),
            parser: Processor::new(),
            sync: SyncFrame::default(),
        }
    }

    pub fn write(&mut self, data: &[u8]) {
        self.sync.scan(data);
        // alacritty ignores a soft reset (DECSTR); xterm.js shows the cursor again and resets the
        // modes. The parts of it that keep the cursor where it is go in at the same point.
        let mut rest = data;
        while let Some(at) = find(rest, SOFT_RESET) {
            let (before, after) = rest.split_at(at + SOFT_RESET.len());
            self.parser.advance(&mut self.term, before);
            self.parser.advance(&mut self.term, SOFT_RESET_PARTS);
            rest = after;
        }
        self.parser.advance(&mut self.term, rest);
    }

    /// The history keeps the same memory at the new width, as the dashboard's does.
    pub fn resize(&mut self, cols: u16, rows: u16) {
        self.term.resize(Size::new(cols, rows));
        self.term.set_options(Config {
            scrolling_history: scrollback_lines(cols),
            ..Config::default()
        });
    }

    pub fn size(&self) -> (u16, u16) {
        let cols = self.term.columns().min(usize::from(u16::MAX)) as u16;
        let rows = self.term.screen_lines().min(usize::from(u16::MAX)) as u16;
        (cols, rows)
    }

    /// True inside a frame that a program draws at once (mode 2026). A snapshot then waits for
    /// the frame to end, up to a limit, so it never shows half a frame.
    pub fn in_sync_frame(&self) -> bool {
        self.sync.open
    }

    /// The screen and the lines above it, as escape sequences for a terminal that was just reset.
    pub fn snapshot(&mut self) -> Vec<u8> {
        serialize(&mut self.term)
    }

    /// The last lines of the screen and the history above it, as plain text, without the empty
    /// rows under the last output.
    pub fn last_lines(&self, max: usize) -> String {
        let grid = self.term.grid();
        let top = grid.topmost_line().0;
        let bottom = grid.bottommost_line().0;
        let line = |row: i32| {
            let cells = &grid[Line(row)];
            let mut text = String::new();
            for column in 0..grid.columns() {
                let cell = &cells[Column(column)];
                if cell
                    .flags
                    .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
                {
                    continue;
                }
                text.push(cell.c);
                text.extend(cell.zerowidth().unwrap_or_default());
            }
            text.trim_end().to_string()
        };
        let mut end = bottom + 1;
        while end > top && line(end - 1).is_empty() {
            end -= 1;
        }
        let start = (end - i32::try_from(max).unwrap_or(i32::MAX)).max(top);
        (start..end).map(line).collect::<Vec<_>>().join("\n")
    }
}

/// The grid size alacritty asks for.
struct Size {
    columns: usize,
    lines: usize,
}

impl Size {
    fn new(cols: u16, rows: u16) -> Size {
        Size {
            columns: usize::from(cols.max(1)),
            lines: usize::from(rows.max(1)),
        }
    }
}

impl Dimensions for Size {
    fn total_lines(&self) -> usize {
        self.lines
    }

    fn screen_lines(&self) -> usize {
        self.lines
    }

    fn columns(&self) -> usize {
        self.columns
    }
}

/// vte holds back the output of a synchronized frame until it ends; xterm.js parses it at once
/// and only delays drawing. The daemon's screen must be like xterm's, so nothing is held back.
#[derive(Default)]
struct Unbuffered;

impl Timeout for Unbuffered {
    fn set_timeout(&mut self, _duration: Duration) {}

    fn clear_timeout(&mut self) {}

    fn pending_timeout(&self) -> bool {
        false
    }
}

const SOFT_RESET: &[u8] = b"\x1b[!p";
/// Cursor shown, insert off, autowrap on, cursor and keypad keys normal, a plain pen.
const SOFT_RESET_PARTS: &[u8] = b"\x1b[?25h\x1b[4l\x1b[?7h\x1b[?1l\x1b>\x1b[0m";

fn find(bytes: &[u8], needle: &[u8]) -> Option<usize> {
    bytes
        .windows(needle.len())
        .position(|window| window == needle)
}

const BEGIN_SYNC: &[u8] = b"\x1b[?2026h";
const END_SYNC: &[u8] = b"\x1b[?2026l";

/// Follows the begin and end of synchronized frames, also when a sequence is cut between reads.
#[derive(Default)]
struct SyncFrame {
    open: bool,
    tail: Vec<u8>,
}

impl SyncFrame {
    fn scan(&mut self, data: &[u8]) {
        let mut bytes = std::mem::take(&mut self.tail);
        bytes.extend_from_slice(&data[data.len().saturating_sub(64 * 1024)..]);
        let mut last: Option<(usize, bool)> = None;
        for start in 0..bytes.len().saturating_sub(BEGIN_SYNC.len() - 1) {
            let window = &bytes[start..start + BEGIN_SYNC.len()];
            if window == BEGIN_SYNC {
                last = Some((start, true));
            } else if window == END_SYNC {
                last = Some((start, false));
            }
        }
        if let Some((_, open)) = last {
            self.open = open;
        }
        let keep = BEGIN_SYNC.len() - 1;
        self.tail = bytes[bytes.len().saturating_sub(keep)..].to_vec();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn follows_synchronized_frames_across_reads() {
        let mut screen = Screen::new(20, 4);
        screen.write(b"a\x1b[?20");
        assert!(!screen.in_sync_frame());
        screen.write(b"26hdraw");
        assert!(screen.in_sync_frame());
        screen.write(b"\x1b[?2026lb");
        assert!(!screen.in_sync_frame());
        // Output in a frame is on the screen at once, as in xterm.js.
        screen.write(b"\x1b[?2026hc");
        assert!(screen.last_lines(5).ends_with("drawbc"));
    }

    #[test]
    fn a_soft_reset_shows_the_cursor_again() {
        let mut screen = Screen::new(20, 4);
        screen.write(b"\x1b[?25l\x1b[4h$ ");
        assert!(String::from_utf8_lossy(&screen.snapshot()).contains("?25l"));
        screen.write(b"\x1b[!p$ ");
        let snapshot = String::from_utf8_lossy(&screen.snapshot()).into_owned();
        assert!(
            !snapshot.contains("?25l") && !snapshot.contains("\x1b[4h"),
            "{snapshot:?}"
        );
        screen.write(b"\x1b[!p\x1b[?25l");
        assert!(String::from_utf8_lossy(&screen.snapshot()).contains("?25l"));
    }

    #[test]
    fn gives_the_last_lines_without_the_empty_rows_under_them() {
        let mut screen = Screen::new(10, 5);
        screen.write("1\r\n2\r\n3 người  \r\n".as_bytes());
        assert_eq!(screen.last_lines(10), "1\n2\n3 người");
        assert_eq!(screen.last_lines(2), "2\n3 người");
        for line in 0..20 {
            screen.write(format!("L{line}\r\n").as_bytes());
        }
        assert_eq!(screen.last_lines(2), "L18\nL19");
        assert_eq!(Screen::new(10, 5).last_lines(3), "");
    }

    #[test]
    fn a_resize_changes_the_size_and_keeps_the_text() {
        let mut screen = Screen::new(10, 5);
        screen.write(b"hello");
        screen.resize(30, 8);
        assert_eq!(screen.size(), (30, 8));
        assert_eq!(screen.last_lines(1), "hello");
    }
}
