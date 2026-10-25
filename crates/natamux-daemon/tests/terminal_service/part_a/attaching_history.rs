//! `describe('attaching to a running shell')`, the cases about a long history.

use natamux_daemon::terminals::limits::OUTPUT_FRAME_BYTES;
use natamux_daemon::terminals::types::Frame;
use natamux_proto::terminals::history::scrollback_lines;

use super::util::{attach_at, attach_one};
use crate::support::{SIZE, Size, numbered, numbered_lines, setup};

#[test]
fn replays_the_whole_history_from_the_first_line() {
    let h = setup();
    attach_one(&h);
    for index in 1..=7000 {
        h.pty(0).emit(format!("line {index}\r\n"));
    }

    let (later, _, _) = attach_one(&h);
    h.settled("T-0001", "1");

    assert_eq!(later.screen(SIZE), numbered_lines(7000));
}

#[test]
fn replays_all_of_5000_5001_and_a_screen_plus_1000_or_1001_lines_past_the_old_limits() {
    let rows = usize::from(SIZE.rows);
    for count in [5000, 5001, rows + 1000, rows + 1001] {
        let h = setup();
        attach_one(&h);
        h.pty(0).emit(numbered(count));

        let (later, _, _) = attach_one(&h);
        h.settled("T-0001", "1");

        assert_eq!(later.screen(SIZE), numbered_lines(count), "{count} lines");
    }
}

#[test]
fn keeps_a_long_agent_session_on_a_wide_screen_as_many_rows_as_fit_the_memory_limit() {
    let wide = Size {
        cols: 200,
        rows: 40,
    };
    let h = setup();
    attach_at(&h, "T-0001", wide, "1");
    let lines: Vec<String> = (1..=10_000)
        .map(|index| format!("{index:05} {}", "z".repeat(180)))
        .collect();
    for line in &lines {
        h.pty(0)
            .emit(format!("\x1b[32m{}\x1b[0m{}\r\n", &line[..5], &line[5..]));
    }

    let (later, _, _) = attach_at(&h, "T-0001", wide, "1");
    h.settled("T-0001", "1");

    // The last row is the empty one the cursor sits on.
    let kept = scrollback_lines(wide.cols) + usize::from(wide.rows) - 1;
    assert_eq!(later.screen(wide), lines[lines.len() - kept..]);
    // Rust: the Node case also looked for the xterm bytes "\x1b[32m10000", which another
    // serializer writes differently, so the colour is not checked here.
    assert!(later.output().contains("10000"));
}

#[test]
fn keeps_fewer_rows_once_the_shell_is_wider_the_newest_ones() {
    let wide = Size {
        cols: 200,
        rows: 24,
    };
    let h = setup();
    attach_one(&h);
    h.pty(0).emit(numbered(7000));

    h.service.resize("T-0001", "1", None, wide);
    let (later, _, _) = attach_at(&h, "T-0001", wide, "1");
    h.settled("T-0001", "1");

    let rows = later.screen(wide);
    assert!(rows.len() <= scrollback_lines(wide.cols) + usize::from(wide.rows));
    assert_eq!(rows.last().map(String::as_str), Some("line 7000"));
    assert!(!rows.contains(&"line 1".to_string()));
}

#[test]
fn gives_every_client_that_attaches_the_whole_history() {
    let h = setup();
    attach_one(&h);
    h.pty(0).emit(numbered(3000));

    let other = Size {
        cols: 100,
        rows: 30,
    };
    let (first, _, _) = attach_one(&h);
    let (second, _, _) = attach_at(&h, "T-0001", other, "1");
    h.settled("T-0001", "1");

    assert_eq!(first.screen(SIZE), numbered_lines(3000));
    assert_eq!(second.screen(other), numbered_lines(3000));
}

#[test]
fn replays_a_long_history_in_pieces_of_a_bounded_size() {
    let h = setup();
    attach_one(&h);
    h.pty(0)
        .emit(format!("{}\r\n", "y".repeat(79)).repeat(5000));

    let (later, _, _) = attach_one(&h);
    h.settled("T-0001", "1");

    let pieces: Vec<usize> = later
        .frames()
        .iter()
        .filter_map(|frame| match frame {
            Frame::Bytes(bytes) => Some(bytes.len()),
            Frame::Message(_) => None,
        })
        .collect();
    assert!(pieces.len() > 1);
    assert!(pieces.iter().all(|piece| *piece <= OUTPUT_FRAME_BYTES));
    assert_eq!(later.screen(SIZE), vec!["y".repeat(79); 5000]);
}
