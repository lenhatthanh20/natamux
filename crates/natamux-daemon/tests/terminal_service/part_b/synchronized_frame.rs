//! `describe('a snapshot during a synchronized frame')`.

use std::sync::Arc;

use natamux_daemon::terminals::limits::SNAPSHOT_SYNC_WAIT_MS;

use super::helpers::kinds;
use crate::support::{FakePty, Harness, Recorder, SIZE, client, setup};

fn shell(output: &str) -> (Harness, FakePty, Arc<Recorder>) {
    let h = setup();
    let (_, first) = client();
    h.attach(&first);
    h.pty(0).emit(output);
    h.advance(0);
    let (later, later_ref) = client();
    h.attach(&later_ref);
    let pty = h.pty(0);
    (h, pty, later)
}

#[test]
fn waits_for_the_frame_to_end() {
    let (h, pty, later) = shell("before\r\n\x1b[?2026h");

    h.advance(50);
    assert_eq!(kinds(&later), ["hello"]);
    pty.emit("drawn\x1b[?2026l");
    h.advance(2);

    assert_eq!(kinds(&later), ["hello", "snapshot", "replayed"]);
    assert!(!later.output().contains("\x1b[?2026"));
    assert_eq!(later.screen(SIZE), ["before", "drawn"]);
}

#[test]
fn takes_the_snapshot_anyway_when_the_frame_does_not_end_in_time() {
    let (h, _, later) = shell("before\r\n\x1b[?2026h");

    h.advance(SNAPSHOT_SYNC_WAIT_MS - 1);
    assert_eq!(kinds(&later), ["hello"]);
    h.advance(1);

    assert_eq!(kinds(&later), ["hello", "snapshot", "replayed"]);
    assert!(!later.output().contains("\x1b[?2026"));
}

#[test]
fn does_not_wait_outside_a_frame() {
    let (h, _, later) = shell("$ ");

    h.advance(0);

    assert_eq!(kinds(&later), ["hello", "snapshot", "replayed"]);
}

// Rust: new case. Output that comes while a snapshot waits is in the snapshot or after it, once.
#[test]
fn output_after_the_frame_reaches_the_client_once() {
    let (h, pty, later) = shell("before\r\n\x1b[?2026h");

    pty.emit("drawn\x1b[?2026l");
    pty.emit("\r\nafter\r\n");
    h.settled("T-0001", "1");

    assert_eq!(later.screen(SIZE), ["before", "drawn", "after"]);
    assert_eq!(later.output().matches("after").count(), 1);
    assert_eq!(later.output().matches("drawn").count(), 1);
}
