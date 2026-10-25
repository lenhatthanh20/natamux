//! `describe('attaching to a running shell')`, the cases about size, modes and short output.

use natamux_proto::terminals::messages::ServerMessage;

use super::util::{attach_at, attach_one};
use crate::support::{SIZE, Size, is_hello, setup};

#[test]
fn sets_the_size_of_the_new_screen_before_it_replays_the_output() {
    let h = setup();
    attach_one(&h);
    h.pty(0).emit("hello");

    attach_at(
        &h,
        "T-0001",
        Size {
            cols: 100,
            rows: 30,
        },
        "1",
    );

    assert_eq!(h.pty(0).sizes(), [(100, 30)]);
}

#[test]
fn does_not_resize_a_shell_that_it_starts() {
    let h = setup();
    attach_one(&h);
    assert!(h.pty(0).sizes().is_empty());
}

#[test]
fn replays_the_last_line_even_without_a_line_break() {
    let h = setup();
    attach_one(&h);
    h.pty(0).emit("line 1\r\n$ ");

    let (later, _, _) = attach_one(&h);
    h.settled("T-0001", "1");

    // Rust: the screen text drops the blank at the end of the last row, as it does for every row.
    assert_eq!(later.screen(SIZE), ["line 1", "$"]);
}

#[test]
fn replays_nothing_for_a_shell_without_output() {
    let h = setup();
    attach_one(&h);

    let (later, _, _) = attach_one(&h);
    h.settled("T-0001", "1");

    assert!(later.bytes().is_empty());
    let controls = later.controls();
    assert!(is_hello(&controls[0]));
    assert_eq!(
        controls[1..],
        [
            ServerMessage::Snapshot { cols: 80, rows: 24 },
            ServerMessage::Replayed { offset: 0 },
        ]
    );
}

#[test]
fn gives_a_returning_client_the_output_printed_while_it_was_away_after_the_old_output() {
    let h = setup();
    let (_, _, detach) = attach_one(&h);
    h.pty(0).emit("before\r\n");
    detach.detach();
    h.pty(0).emit("while away\r\n");

    let (back, _, _) = attach_one(&h);
    h.settled("T-0001", "1");

    assert_eq!(back.screen(SIZE), ["before", "while away"]);
}

#[test]
fn joins_the_replay_and_the_live_output_with_no_line_lost_or_shown_twice() {
    let h = setup();
    let (watching, _, _) = attach_one(&h);
    h.pty(0).emit(crate::support::numbered(2000));

    let (later, _, _) = attach_one(&h);
    h.pty(0).emit("live\r\n");
    h.settled("T-0001", "1");

    let mut expected = crate::support::numbered_lines(2000);
    expected.push("live".into());
    assert_eq!(later.screen(SIZE), expected);
    assert_eq!(watching.screen(SIZE), expected);
}

// An agent places text with cursor moves for the width it had. The raw output on a narrower
// screen puts the first letter of each word at the right edge and the rest on the next row.
#[test]
fn draws_the_screen_again_for_the_size_of_a_client_that_attaches_later() {
    let h = setup();
    attach_at(
        &h,
        "T-0001",
        Size {
            cols: 120,
            rows: 24,
        },
        "1",
    );
    h.pty(0)
        .emit("Spec mới: panel sẽ mở\x1b[100GNếu\x1b[105Ganh\r\nnext\r\n");

    let (later, _, _) = attach_one(&h);
    h.settled("T-0001", "1");

    assert_eq!(
        later.screen(SIZE),
        [
            "Spec mới: panel sẽ mở".to_string(),
            format!("{}Nếu  anh", " ".repeat(19)),
            "next".to_string(),
        ]
    );
}

// An agent counts an emoji as two cells, as the dashboard does. With the old Unicode 6 widths,
// text placed after it with a cursor move left a gap.
#[test]
fn gives_an_emoji_two_cells_as_the_dashboard_does() {
    let h = setup();
    attach_one(&h);
    h.pty(0).emit("a😀b\x1b[5Gx\r\n");

    let (later, _, _) = attach_one(&h);
    h.settled("T-0001", "1");

    assert_eq!(later.screen(SIZE), ["a😀bx"]);
}

#[test]
fn keeps_the_modes_a_program_turned_on_such_as_bracketed_paste() {
    let h = setup();
    attach_one(&h);
    h.pty(0).emit("\x1b[?2004h$ ");

    let (later, _, _) = attach_one(&h);
    h.settled("T-0001", "1");

    assert!(later.output().contains("\x1b[?2004h"));
}
