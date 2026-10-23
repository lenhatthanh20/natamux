//! `describe('a sliced snapshot')`. Rust has no slices. The cases whose point is the client's final
//! screen are kept: the client draws the same screen as the output gives, cursor and styles
//! included. Left out: output or a resize between slices, other shells moving during a slice, and
//! one-piece versus sliced serializing (no slicing).

use super::helpers::{assert_same_screen, mixed_lines};
use crate::support::{SIZE, Size, client, numbered, numbered_lines, setup};
use natamux_proto::terminals::history::scrollback_lines;

fn later_screen(output: &str, screen: Size) -> std::sync::Arc<crate::support::Recorder> {
    let h = setup();
    let (_, first) = client();
    h.attach_to("T-0001", &first, screen, "1", Default::default());
    h.pty(0).emit(output);
    h.settled("T-0001", "1");

    let (later, later_ref) = client();
    h.attach_to("T-0001", &later_ref, screen, "1", Default::default());
    h.settled("T-0001", "1");
    later
}

fn full_history(cols: u16) {
    let screen = Size { cols, rows: 30 };
    let output = mixed_lines(usize::from(cols), scrollback_lines(cols) + 500);
    let later = later_screen(&output, screen);
    assert_same_screen(&later, &output, screen);
}

#[test]
fn serializes_a_full_history_of_80_columns_the_same_as_in_one_piece() {
    full_history(80);
}

#[test]
fn serializes_a_full_history_of_120_columns_the_same_as_in_one_piece() {
    full_history(120);
}

#[test]
fn serializes_a_full_history_of_200_columns_the_same_as_in_one_piece() {
    full_history(200);
}

#[test]
fn keeps_colored_rows_right() {
    let colored: String = (0..1500)
        .map(|index| match index % 3 {
            0 => format!("\x1b[42m+ added {index}\x1b[K\x1b[0m\r\n"),
            1 => format!("\x1b[44m{}\x1b[0m\r\n", " ".repeat(80)),
            _ => format!("plain {index}\r\n"),
        })
        .collect();
    let output = format!("{}{colored}{}", numbered(60), numbered(60));
    let later = later_screen(&output, SIZE);
    assert_same_screen(&later, &output, SIZE);
}

#[test]
fn starts_the_last_part_at_a_row_that_is_not_a_continuation() {
    let tail: String = (0..SIZE.rows - 2)
        .map(|index| format!("tail {index}\r\n"))
        .collect();
    // The long line wraps over three rows; the screen starts at its last row.
    let output = format!(
        "{}{}\r\n{tail}",
        numbered(2000),
        "w".repeat(2 * usize::from(SIZE.cols) + 5)
    );
    let later = later_screen(&output, SIZE);
    assert_same_screen(&later, &output, SIZE);
}

#[test]
fn sends_the_alternate_screen_and_the_modes_once() {
    let output = format!(
        "{}\x1b[?1049h\x1b[?2004h\x1b[?1000h\x1b[?1h\x1b[Hfull screen app",
        numbered(2000)
    );
    let later = later_screen(&output, SIZE);

    let sent = later.output();
    for mode in ["\x1b[?1049h", "\x1b[?2004h", "\x1b[?1000h", "\x1b[?1h"] {
        assert_eq!(sent.split(mode).count(), 2, "{mode:?}");
    }
    assert_same_screen(&later, &output, SIZE);
}

fn hidden_cursor(lines: usize) {
    let output = format!("\x1b[?25l{}", numbered(lines));
    let later = later_screen(&output, SIZE);

    let sent = later.output();
    // Rust: the snapshot need not end with the sequence; the last cursor mode it sets must hide it.
    let hidden = sent.rfind("\x1b[?25l");
    assert!(hidden.is_some());
    assert!(hidden > sent.rfind("\x1b[?25h"));
    assert_eq!(later.screen(SIZE), numbered_lines(lines));
}

#[test]
fn keeps_a_hidden_cursor_hidden_in_one_piece_once_at_the_end() {
    hidden_cursor(100);
}

#[test]
fn keeps_a_hidden_cursor_hidden_in_many_slices_once_at_the_end() {
    hidden_cursor(2000);
}
