//! `describe('attaching to a running shell')` > `describe('a cursor a program hid')`.
//!
//! Rust: xterm is not here, so "hidden after the output" is read from the last `?25l` or `?25h`
//! in the snapshot bytes (a snapshot holds no reset).

use super::util::attach_one;
use crate::support::{Recorder, SIZE, setup};

const HIDE: &str = "\x1b[?25l";
const SHOW: &str = "\x1b[?25h";

/// The snapshot a client attached after these reads of the shell gets.
fn snapshot_after(reads: &[&str]) -> (std::sync::Arc<Recorder>, String) {
    let h = setup();
    attach_one(&h);
    for read in reads {
        h.pty(0).emit(read);
    }
    let (later, _, _) = attach_one(&h);
    h.settled("T-0001", "1");
    let output = later.output();
    (later, output)
}

fn hidden_after(output: &str) -> bool {
    match (output.rfind(HIDE), output.rfind(SHOW)) {
        (Some(hide), Some(show)) => hide > show,
        (hide, _) => hide.is_some(),
    }
}

#[test]
fn stays_hidden_when_hidden() {
    let cases: [(&str, &[&str]); 6] = [
        ("with the text", &["\x1b[?25l→ Add a follow-up"]),
        ("before the text", &["\x1b[?25l", "→ Add a follow-up"]),
        (
            "split across two reads",
            &["\x1b[?2", "5l→ Add a follow-up"],
        ),
        (
            "with other modes in the same sequence",
            &["\x1b[?1049;25l→ Add a follow-up"],
        ),
        (
            "again after it showed it",
            &["\x1b[?25l", "\x1b[?25h", "\x1b[?25l$ "],
        ),
        (
            "again after a reset",
            &["\x1b[?25l", "\x1bc", "\x1b[?25l$ "],
        ),
    ];
    for (name, reads) in cases {
        let (_, output) = snapshot_after(reads);

        assert!(output.ends_with(HIDE), "{name}: {output:?}");
        assert!(hidden_after(&output), "{name}");
    }
}

#[test]
fn is_hidden_once_in_the_snapshot_however_many_times_the_program_hid_it() {
    let (_, output) = snapshot_after(&["\x1b[?25l", "\x1b[?25lwork", "\x1b[?25l$ "]);

    assert_eq!(output.split(HIDE).count(), 2);
}

#[test]
fn shows_when_the_program_shows_it_again_or_resets() {
    let cases: [(&str, &[&str]); 3] = [
        ("shows it again", &["\x1b[?25l", "\x1b[?25h$ "]),
        (
            "shows it again with other modes",
            &["\x1b[?25l", "\x1b[?25;1049h$ "],
        ),
        ("resets (RIS)", &["\x1b[?25l", "\x1bc$ "]),
    ];
    for (name, reads) in cases {
        let (_, output) = snapshot_after(reads);

        assert!(!output.contains(HIDE), "{name}: {output:?}");
        assert!(!hidden_after(&output), "{name}");
    }
}

#[test]
fn shows_when_the_program_soft_resets_decstr() {
    let (_, output) = snapshot_after(&["\x1b[?25l", "\x1b[!p$ "]);

    assert!(!output.contains(HIDE), "{output:?}");
    assert!(!hidden_after(&output));
}

#[test]
fn stays_shown_after_other_modes_or_a_show_or_a_reset_with_nothing_hidden() {
    let cases = [
        ("other private modes", "\x1b[?1049l\x1b[?7l"),
        ("modes whose number holds 25", "\x1b[?2025l\x1b[?125h"),
        ("a mode 25 that is not private", "\x1b[25l"),
        ("a show with nothing hidden", "\x1b[?25h"),
        ("a reset with nothing hidden", "\x1bc"),
    ];
    for (name, read) in cases {
        let (_, output) = snapshot_after(&[&format!("{read}$ ")]);

        assert!(!output.contains(HIDE), "{name}: {output:?}");
        assert!(!hidden_after(&output), "{name}");
    }
}

// The alt screen keeps the cursor column, hence the \r.
#[test]
fn still_lets_the_screen_handle_the_sequences_it_watches() {
    let (in_alt, output) = snapshot_after(&["\x1b[?25lmain", "\x1b[?1049;25h", "\ralt"]);
    assert_eq!(in_alt.screen(SIZE), ["alt"]);
    assert!(output.contains("\x1b[?1049h"));
    assert!(!hidden_after(&output));

    let (back, output) = snapshot_after(&["main", "\x1b[?1049;25h", "\ralt", "\x1b[?1049;25l"]);
    assert_eq!(back.screen(SIZE), ["main"]);
    assert!(!output.contains("\x1b[?1049h"));
    assert!(hidden_after(&output));
}
