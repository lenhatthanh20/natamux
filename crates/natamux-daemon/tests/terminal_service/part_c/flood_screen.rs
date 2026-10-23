//! `describe('the screen of a shell under a flood')`.
//!
//! Left out: the four cases about the pty pause by screen marks (SCREEN_HIGH_WATER and
//! SCREEN_LOW_WATER) and the one about output that waits during a sliced snapshot. The Rust screen
//! parses at once, so nothing pauses the pty. The cases below keep what still holds.

use super::util::{MAIN, TASK, Watcher, adv, daemon_screen, expect_same_screen, frame_of};
use crate::support::{client, setup};

// Rust: replaces 'pauses the shell when its screen falls behind': nothing pauses, nothing is lost.
#[test]
fn a_flood_of_a_few_megabytes_pauses_nothing_and_loses_no_output() {
    let h = setup();
    let (watching, _detach) = Watcher::attach(&h, MAIN, true);
    let pty = h.pty(0);
    let chunk = format!("{}{}", "中".repeat(10_000), "a".repeat(30_000));

    // Bytes, not chars: each chunk is 60,000 bytes.
    for _ in 0..20 {
        pty.emit(&chunk);
        adv(&h, &[&watching], 5);
    }
    assert!(pty.state().flow.is_empty());

    // Ctrl+C still reaches the program.
    h.service.input(TASK, MAIN, None, "\x03");
    assert_eq!(pty.written(), ["\x03"]);

    h.settled(TASK, MAIN);
    assert!(pty.state().flow.is_empty());
    assert!(!pty.state().paused);
    // The fake pty echoes the Ctrl+C.
    assert!(watching.output() == format!("{}\x03", chunk.repeat(20)));
}

// Rust: replaces 'counts the output that waits during a snapshot': a client that attaches during a
// flood gets the output once, and ends on the daemon's screen.
#[test]
fn a_client_that_attaches_during_a_flood_ends_on_the_screen_of_the_daemon() {
    let h = setup();
    let (_, first) = client();
    h.attach(&first);
    let pty = h.pty(0);
    for index in 0..8 {
        pty.emit(frame_of(index));
    }
    h.settled(TASK, MAIN);

    let (later, _detach) = Watcher::attach(&h, MAIN, true);
    for index in 8..40 {
        pty.emit(frame_of(index));
        adv(&h, &[&later], 5);
    }
    adv(&h, &[&later], 1000);
    assert!(pty.state().flow.is_empty());

    let daemon = daemon_screen(&h, &[&later]);
    expect_same_screen(&later.frames(), &daemon);
}
