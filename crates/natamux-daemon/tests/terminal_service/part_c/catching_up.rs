//! `describe('catching up a client that is behind')`.
//!
//! Left out: 'sends the snapshot alone when too much output waited, then catches up'. It needs a
//! sliced snapshot with output between the slices, which the Rust service does not have.

use std::sync::Arc;

use natamux_daemon::terminals::limits::{
    ACK_STALL_MS, CLIENT_HIGH_WATER, HIDDEN_BUDGET_BYTES, OUTPUT_FRAME_BYTES, OUTPUT_RING_BYTES,
    RESYNC_MAX_WAIT_MS, RESYNC_QUIET_WINDOW_MS,
};
use natamux_daemon::terminals::service::AttachOptions;
use natamux_daemon::terminals::types::ClientRef;
use natamux_proto::terminals::messages::ServerMessage;

use super::util::{
    MAIN, TASK, Watcher, adv, catch_ups, daemon_screen, expect_same_screen, flood, frame_of,
    sent_to, since_snapshot, size_of, snapshot_msg,
};
use crate::support::{
    FakePty, Harness, Recorder, SIZE, bytes_of, client, controls, is_hello, setup,
};

const FRAME: u64 = OUTPUT_FRAME_BYTES as u64;
const RING_FRAMES: usize = OUTPUT_RING_BYTES / OUTPUT_FRAME_BYTES;

fn resume() -> Vec<ServerMessage> {
    vec![ServerMessage::Resume {
        offset: CLIENT_HIGH_WATER,
    }]
}

/// A visible client that got the first 8 of `count` frames, missed the rest, and acked what it got.
fn behind(count: usize) -> (Harness, FakePty, Arc<Recorder>, ClientRef) {
    let h = setup();
    let (slow, slow_ref) = client();
    h.attach(&slow_ref);
    let pty = h.pty(0);
    for index in 0..count {
        pty.emit(frame_of(index));
    }
    h.service.ack(TASK, MAIN, &slow_ref, CLIENT_HIGH_WATER);
    (h, pty, slow, slow_ref)
}

fn quiet(h: &Harness, ms: u64) {
    adv(h, &[], ms);
}

#[test]
fn waits_while_the_shell_floods_until_it_has_been_behind_resync_max_wait_ms() {
    let (h, pty, slow, _) = behind(9);

    // 1000 bytes per 10 ms is more than cols * rows / 8 per RESYNC_QUIET_WINDOW_MS.
    flood(&h, &[], &pty, RESYNC_MAX_WAIT_MS - 10, 1000);
    assert_eq!(catch_ups(&slow.frames()), []);
    flood(&h, &[], &pty, 20, 1000);
    assert_eq!(catch_ups(&slow.frames()), resume());
    flood(&h, &[], &pty, RESYNC_MAX_WAIT_MS, 1000);
    assert_eq!(catch_ups(&slow.frames()).len(), 1);
}

#[test]
fn starts_one_quiet_resync_quiet_window_ms_after_the_flood_stops() {
    let (h, pty, slow, _) = behind(9);

    flood(&h, &[], &pty, 500, 1000);
    // The last output was 10 ms ago.
    quiet(&h, RESYNC_QUIET_WINDOW_MS - 11);
    assert_eq!(catch_ups(&slow.frames()), []);
    quiet(&h, 1);
    assert_eq!(catch_ups(&slow.frames()), resume());
}

#[test]
fn catches_up_an_acking_client_once_the_flood_stops_with_no_output_after_it() {
    let h = setup();
    let (watching, _detach) = Watcher::attach(&h, MAIN, true);
    adv(&h, &[&watching], 0);
    let pty = h.pty(0);
    // More at once than it may have unacked.
    for index in 0..9 {
        pty.emit(frame_of(index));
    }

    flood(&h, &[&watching], &pty, 500, 1000);
    assert_eq!(catch_ups(&watching.frames()), []);
    adv(&h, &[&watching], RESYNC_QUIET_WINDOW_MS);

    assert_eq!(catch_ups(&watching.frames()), resume());
}

#[test]
fn gives_a_client_whose_acks_stopped_a_snapshot_after_ack_stall_ms_of_quiet() {
    let h = setup();
    let (stuck, stuck_ref) = client();
    h.attach(&stuck_ref);
    let pty = h.pty(0);
    // Behind with 512 KB unacked, and it never acks again.
    for index in 0..9 {
        pty.emit(frame_of(index));
    }

    quiet(&h, ACK_STALL_MS - 1);
    assert_eq!(catch_ups(&stuck.frames()), []);
    quiet(&h, 500);

    assert_eq!(catch_ups(&stuck.frames()), [snapshot_msg()]);
    assert_eq!(
        controls(&stuck.frames()).last(),
        Some(&ServerMessage::Replayed { offset: 9 * FRAME })
    );
}

// Rust: no xterm to parse the output, so the sequence is looked for in the bytes.
#[test]
fn keeps_a_cursor_the_program_hid_hidden_in_a_catch_up_snapshot() {
    let h = setup();
    let (stuck, stuck_ref) = client();
    h.attach(&stuck_ref);
    let pty = h.pty(0);
    pty.emit("\u{1b}[?25l");
    for index in 0..9 {
        pty.emit(frame_of(index));
    }

    quiet(&h, ACK_STALL_MS + 500);

    assert_eq!(catch_ups(&stuck.frames()), [snapshot_msg()]);
    assert!(matches!(
        controls(&stuck.frames()).last(),
        Some(ServerMessage::Replayed { .. })
    ));
    let output = bytes_of(&since_snapshot(&stuck.frames()));
    assert!(output.ends_with(b"\x1b[?25l"));
}

#[test]
fn does_not_give_a_hidden_client_whose_acks_stopped_a_snapshot() {
    let h = setup();
    let (stuck, stuck_ref) = client();
    h.attach_to(
        TASK,
        &stuck_ref,
        SIZE,
        MAIN,
        AttachOptions {
            visible: Some(false),
            ..AttachOptions::default()
        },
    );
    let pty = h.pty(0);
    for index in 0..=(HIDDEN_BUDGET_BYTES / FRAME) as usize {
        pty.emit(frame_of(index));
    }

    quiet(&h, 3 * ACK_STALL_MS);

    assert_eq!(catch_ups(&stuck.frames()), []);
}

#[test]
fn takes_output_of_exactly_cols_rows_8_in_the_window_as_not_quiet() {
    let (h, pty, slow, _) = behind(9);

    quiet(&h, RESYNC_QUIET_WINDOW_MS / 2);
    pty.emit("q".repeat(usize::from(SIZE.cols) * usize::from(SIZE.rows) / 8));
    quiet(&h, RESYNC_QUIET_WINDOW_MS - 1);
    assert_eq!(catch_ups(&slow.frames()), []);
    quiet(&h, 1);
    assert_eq!(catch_ups(&slow.frames()), resume());
}

#[test]
fn measures_quiet_against_the_size_the_shell_has_now() {
    let (h, pty, slow, slow_ref) = behind(9);

    flood(&h, &[], &pty, 500, 100);
    assert_eq!(catch_ups(&slow.frames()), []);
    // 1000 bytes per window is less than 200 * 50 / 8.
    h.service
        .resize(TASK, MAIN, Some(&slow_ref), size_of(200, 50));
    flood(&h, &[], &pty, RESYNC_QUIET_WINDOW_MS, 100);

    assert_eq!(catch_ups(&slow.frames()), resume());
}

#[test]
fn resumes_from_the_ring_while_it_still_has_what_the_client_missed_even_from_its_first_byte() {
    // The ring holds the last 16 frames, so the client's first missed byte starts it.
    let (h, _pty, slow, _) = behind(8 + RING_FRAMES);

    quiet(&h, RESYNC_QUIET_WINDOW_MS);

    assert_eq!(catch_ups(&slow.frames()), resume());
    assert_eq!(
        controls(&slow.frames()).last(),
        Some(&ServerMessage::Replayed { offset: 24 * FRAME })
    );
    let daemon = daemon_screen(&h, &[]);
    expect_same_screen(&slow.frames(), &daemon);
}

#[test]
fn takes_a_snapshot_once_the_ring_lost_what_the_client_missed() {
    let (h, _pty, slow, _) = behind(9 + RING_FRAMES);

    // The shell is quiet: no wait, though the snapshot of its attach was just now.
    quiet(&h, RESYNC_QUIET_WINDOW_MS - 1);
    assert_eq!(catch_ups(&slow.frames()), []);
    quiet(&h, 500);

    assert_eq!(catch_ups(&slow.frames()), [snapshot_msg()]);
    assert_eq!(
        controls(&slow.frames()).last(),
        Some(&ServerMessage::Replayed { offset: 25 * FRAME })
    );
    let daemon = daemon_screen(&h, &[]);
    expect_same_screen(&since_snapshot(&slow.frames()), &daemon);
}

/// Behind again right after a catch-up snapshot, with a gap the ring lost.
fn fall_behind_again(h: &Harness, pty: &FakePty, slow: &Recorder, slow_ref: &ClientRef) {
    for index in 0..9 + RING_FRAMES {
        pty.emit(frame_of(index));
    }
    h.service.ack(TASK, MAIN, slow_ref, sent_to(&slow.frames()));
}

#[test]
fn sends_a_shell_that_keeps_flooding_at_most_one_snapshot_per_resync_max_wait_ms() {
    let (h, pty, slow, slow_ref) = behind(9 + RING_FRAMES);
    flood(&h, &[], &pty, RESYNC_MAX_WAIT_MS + 50, 1000);
    assert_eq!(catch_ups(&slow.frames()), [snapshot_msg()]);

    fall_behind_again(&h, &pty, &slow, &slow_ref);
    flood(&h, &[], &pty, RESYNC_MAX_WAIT_MS - 100, 1000);
    assert_eq!(catch_ups(&slow.frames()).len(), 1);
    flood(&h, &[], &pty, 200, 1000);
    assert_eq!(catch_ups(&slow.frames()).len(), 2);
}

#[test]
fn sends_the_snapshot_at_once_when_the_flood_stops_as_after_a_ctrl_c() {
    let (h, pty, slow, slow_ref) = behind(9 + RING_FRAMES);
    flood(&h, &[], &pty, RESYNC_MAX_WAIT_MS + 50, 1000);
    assert_eq!(catch_ups(&slow.frames()).len(), 1);

    // The last snapshot was 50 ms ago; the shell is quiet from here on.
    fall_behind_again(&h, &pty, &slow, &slow_ref);
    quiet(&h, RESYNC_QUIET_WINDOW_MS - 1);
    assert_eq!(catch_ups(&slow.frames()).len(), 1);
    quiet(&h, 100);
    assert_eq!(catch_ups(&slow.frames()), [snapshot_msg(), snapshot_msg()]);
    assert!(is_hello(&controls(&slow.frames())[0]));
}
