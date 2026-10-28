//! The catch-up snapshot of T-0223 P8 against the full one, at 80 cols as the spec measures it:
//! a catch-up sends only the newest `CATCH_UP_HISTORY_ROWS` of the history.

use std::time::Instant;

use natamux_proto::terminals::history::{CATCH_UP_HISTORY_ROWS, scrollback_lines};
use natamux_screen::Screen;

use super::ms;

const CATCH_UP_COLS: u16 = 80;
const CATCH_UP_ROWS: u16 = 24;

/// The full snapshot and the catch-up one of a screen whose history is full: (KiB, ms) each.
pub fn catch_up() -> ((f64, f64), (f64, f64)) {
    let mut screen = Screen::new(CATCH_UP_COLS, CATCH_UP_ROWS);
    screen.write(&full_history());
    let start = Instant::now();
    let full = screen.snapshot();
    let full_ms = ms(start.elapsed());
    let start = Instant::now();
    let tail = screen.snapshot_tail(CATCH_UP_HISTORY_ROWS);
    let tail_ms = ms(start.elapsed());
    // The P8 target: a catch-up at least 5 times smaller than the full snapshot.
    assert!(
        tail.len() * 5 <= full.len(),
        "the catch-up is {} bytes of {}",
        tail.len(),
        full.len()
    );
    ((kib(full.len()), full_ms), (kib(tail.len()), tail_ms))
}

fn full_history() -> Vec<u8> {
    let lines = scrollback_lines(CATCH_UP_COLS) + usize::from(CATCH_UP_ROWS);
    let mut text = String::new();
    for index in 0..lines {
        text += &format!("\x1b[3{}m{index:06}\x1b[0m ", index % 8);
        text += &"x".repeat(usize::from(CATCH_UP_COLS) - 12);
        text += "\r\n";
    }
    text.into_bytes()
}

fn kib(bytes: usize) -> f64 {
    (bytes as f64 / 1024.0 * 10.0).round() / 10.0
}
