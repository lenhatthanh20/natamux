//! `describe('a client that comes back')`.

use std::sync::Arc;

use natamux_daemon::terminals::limits::{OUTPUT_FRAME_BYTES, OUTPUT_RING_BYTES};
use natamux_daemon::terminals::service::AttachOptions;
use natamux_daemon::terminals::types::{Frame, TerminalError};
use natamux_proto::terminals::messages::ServerMessage;

use super::util::{
    MAIN, TASK, daemon_screen, expect_same_screen, frame_of, sent_to, size_of, snapshot_msg,
};
use crate::support::{FakePty, Harness, Recorder, SIZE, bytes_of, client, controls, setup};

struct Lost {
    h: Harness,
    pty: FakePty,
    away: Arc<Recorder>,
    shell_id: String,
    offset: u64,
}

fn shell_id_of(frames: &[Frame]) -> String {
    match controls(frames).first() {
        Some(ServerMessage::Hello { shell_id }) => shell_id.clone(),
        other => panic!("No hello came first: {other:?}"),
    }
}

/// A client that saw the screen so far, then lost its socket.
fn lost() -> Lost {
    let h = setup();
    let (away, away_ref) = client();
    let detach = h.attach(&away_ref);
    let pty = h.pty(0);
    pty.emit("before\r\n");
    h.settled(TASK, MAIN);
    detach.detach();
    let frames = away.frames();
    Lost {
        shell_id: shell_id_of(&frames),
        offset: sent_to(&frames),
        h,
        pty,
        away,
    }
}

fn resume_opts(shell_id: &str, offset: Option<u64>) -> AttachOptions {
    AttachOptions {
        shell_id: Some(shell_id.into()),
        offset,
        ..AttachOptions::default()
    }
}

fn hello(shell_id: &str) -> ServerMessage {
    ServerMessage::Hello {
        shell_id: shell_id.into(),
    }
}

// AC 27, C29
#[test]
fn resumes_from_the_ring_with_only_the_bytes_it_missed_with_no_snapshot() {
    let Lost {
        h,
        pty,
        away,
        shell_id,
        offset,
    } = lost();
    let missed = "missed 1\r\nmissed 2 é\r\n";
    pty.emit(missed);

    let (back, back_ref) = client();
    h.attach_to(
        TASK,
        &back_ref,
        SIZE,
        MAIN,
        resume_opts(&shell_id, Some(offset)),
    );

    let end = offset + missed.len() as u64;
    assert_eq!(
        back.controls(),
        [
            hello(&shell_id),
            ServerMessage::Resume { offset },
            ServerMessage::Replayed { offset: end },
        ]
    );
    assert_eq!(back.output(), missed);
    assert_eq!(h.ptys().len(), 1);
    // Live output follows, and its screen is the daemon's.
    pty.emit("live\r\n");
    h.settled(TASK, MAIN);
    assert_eq!(back.output(), format!("{missed}live\r\n"));
    let both: Vec<Frame> = [away.frames(), back.frames()].concat();
    expect_same_screen(&both, &daemon_screen(&h, &[]));
}

#[test]
fn resumes_with_no_bytes_when_nothing_was_missed() {
    let Lost {
        h,
        shell_id,
        offset,
        ..
    } = lost();
    let (back, back_ref) = client();
    h.attach_to(
        TASK,
        &back_ref,
        SIZE,
        MAIN,
        resume_opts(&shell_id, Some(offset)),
    );

    assert_eq!(
        back.frames(),
        [
            Frame::Message(hello(&shell_id)),
            Frame::Message(ServerMessage::Resume { offset }),
            Frame::Message(ServerMessage::Replayed { offset }),
        ]
    );
}

// AC 28, C30
#[test]
fn takes_the_snapshot_path_for_an_offset_the_ring_no_longer_has_or_never_had() {
    let Lost {
        h, pty, shell_id, ..
    } = lost();
    let frames = OUTPUT_RING_BYTES / OUTPUT_FRAME_BYTES * 5 / 4;
    for index in 0..frames {
        pty.emit(frame_of(index));
    }
    h.settled(TASK, MAIN);
    let end = "before\r\n".len() as u64 + (frames * OUTPUT_FRAME_BYTES) as u64;
    let ring_start = end - OUTPUT_RING_BYTES as u64;
    let daemon = daemon_screen(&h, &[]);

    // Too old, in the future, inside a frame.
    for offset in [0, end + 1, ring_start + 10] {
        let (back, back_ref) = client();
        h.attach_to(
            TASK,
            &back_ref,
            SIZE,
            MAIN,
            resume_opts(&shell_id, Some(offset)),
        );
        h.settled(TASK, MAIN);
        assert_eq!(
            back.controls(),
            [
                hello(&shell_id),
                snapshot_msg(),
                ServerMessage::Replayed { offset: end },
            ]
        );
        expect_same_screen(&back.frames(), &daemon);
    }
    // Exactly at the start of the ring.
    let (edge, edge_ref) = client();
    h.attach_to(
        TASK,
        &edge_ref,
        SIZE,
        MAIN,
        resume_opts(&shell_id, Some(ring_start)),
    );
    assert_eq!(
        edge.controls()[1],
        ServerMessage::Resume { offset: ring_start }
    );
    assert_eq!(bytes_of(&edge.frames()).len(), OUTPUT_RING_BYTES);
    assert_eq!(h.ptys().len(), 1);
}

// AC 43, C45 (b)
#[test]
fn takes_the_snapshot_path_for_a_resume_without_an_offset_and_starts_no_shell() {
    let Lost {
        h,
        pty,
        away,
        shell_id,
        ..
    } = lost();
    pty.emit("while away\r\n");

    let (back, back_ref) = client();
    h.attach_to(TASK, &back_ref, SIZE, MAIN, resume_opts(&shell_id, None));
    h.settled(TASK, MAIN);

    assert_eq!(
        back.controls(),
        [
            hello(&shell_id),
            snapshot_msg(),
            ServerMessage::Replayed {
                offset: sent_to(&away.frames()) + "while away\r\n".len() as u64
            },
        ]
    );
    assert_eq!(back.screen(SIZE), ["before", "while away"]);
    assert_eq!(h.ptys().len(), 1);
}

fn is_gone(result: Result<impl Sized, TerminalError>) -> bool {
    matches!(result, Err(TerminalError::ShellGone { .. }))
}

// AC 29, C31
#[test]
fn refuses_a_resume_of_a_shell_that_is_gone_or_was_started_again_and_starts_none() {
    let h = setup();
    let (none, none_ref) = client();
    assert!(is_gone(h.service.attach(
        TASK,
        none_ref,
        SIZE,
        MAIN,
        resume_opts("old", Some(0))
    )));
    assert_eq!(h.ptys().len(), 0);
    assert!(none.frames().is_empty());

    let (running, running_ref) = client();
    h.attach(&running_ref);
    let shell_id = shell_id_of(&running.frames());
    let (other, other_ref) = client();
    let wrong = format!("{shell_id}x");
    assert!(is_gone(h.service.attach(
        TASK,
        other_ref,
        SIZE,
        MAIN,
        resume_opts(&wrong, None)
    )));
    assert!(other.frames().is_empty());

    h.pty(0).exit(Some(0));
    let (_, again_ref) = client();
    assert!(is_gone(h.service.attach(
        TASK,
        again_ref,
        SIZE,
        MAIN,
        resume_opts(&shell_id, None)
    )));
    assert!(!h.service.is_open(TASK, MAIN));
    assert_eq!(h.ptys().len(), 1);
}

// AC 42, C44: a resume on screen still does not claim, and never gets the size back.
#[test]
fn never_takes_the_size_for_a_resume_even_on_screen() {
    let h = setup();
    let large = size_of(120, 40);
    let middle = size_of(100, 30);
    let (watching, watching_ref) = client();
    h.attach_to(TASK, &watching_ref, middle, MAIN, AttachOptions::default());
    let (_, large_ref) = client();
    let detach = h.attach_to(TASK, &large_ref, large, MAIN, AttachOptions::default());
    h.settled(TASK, MAIN);
    let shell_id = shell_id_of(&watching.frames());

    let (_, resumed_ref) = client();
    h.attach_to(
        TASK,
        &resumed_ref,
        SIZE,
        MAIN,
        resume_opts(&shell_id, Some(0)),
    );
    assert_eq!(h.pty(0).sizes(), [(120, 40)]);
    detach.detach();

    assert_eq!(h.pty(0).sizes(), [(120, 40), (100, 30)]);
}
