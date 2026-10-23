//! `describe('output before control messages')`.

use std::sync::Arc;

use natamux_daemon::terminals::types::ClientRef;
use natamux_proto::terminals::messages::ServerMessage;

use crate::support::{FakePty, Harness, Recorder, SIZE, Size, bytes_of, client, setup};

struct Watched {
    h: Harness,
    pty: FakePty,
    watching: Arc<Recorder>,
    other_ref: ClientRef,
}

fn watched() -> Watched {
    let h = setup();
    let (watching, watching_ref) = client();
    let (_, other_ref) = client();
    h.attach(&watching_ref);
    h.attach(&other_ref);
    h.settled("T-0001", "1");
    let pty = h.pty(0);
    Watched {
        h,
        pty,
        watching,
        other_ref,
    }
}

/// The text of the frame before the last one.
fn before_last(client: &Recorder) -> String {
    let frames = client.frames();
    String::from_utf8_lossy(&bytes_of(&frames[frames.len() - 2..frames.len() - 1])).into_owned()
}

#[test]
fn sends_pending_output_before_the_exit_of_the_shell() {
    let w = watched();

    w.pty.emit("bye");
    w.pty.exit(Some(0));

    assert_eq!(before_last(&w.watching), "bye");
    assert_eq!(
        w.watching.controls().last(),
        Some(&ServerMessage::Exit { code: Some(0) })
    );
}

#[test]
fn sends_pending_output_before_the_shell_is_closed() {
    let w = watched();

    w.pty.emit("bye");
    w.h.service.close_one("T-0001", "1");
    w.h.pump();

    assert_eq!(before_last(&w.watching), "bye");
    assert_eq!(
        w.watching.controls().last(),
        Some(&ServerMessage::Exit { code: None })
    );
}

#[test]
fn sends_pending_output_before_the_shell_is_resized() {
    let w = watched();

    w.pty.emit("drawn for 80 columns");
    // Rust: the fake pty cannot look at the client when it is resized, so check that the output
    // is sent when the resize call returns, and that the pty got the new size after it.
    w.h.service.resize(
        "T-0001",
        "1",
        Some(&w.other_ref),
        Size {
            cols: 100,
            rows: 30,
        },
    );

    assert_eq!(w.watching.output(), "drawn for 80 columns");
    assert_eq!(w.pty.sizes(), [(100, 30)]);
}

#[test]
fn sends_nothing_early_for_a_resize_to_the_same_size() {
    let w = watched();

    w.pty.emit("abc");
    w.h.service.resize("T-0001", "1", Some(&w.other_ref), SIZE);

    assert!(w.pty.sizes().is_empty());
    assert_eq!(w.watching.output(), "");
    w.h.settled("T-0001", "1");
    assert_eq!(w.watching.output(), "abc");
}

#[test]
fn sends_pending_output_to_the_clients_when_a_new_client_takes_its_snapshot() {
    let w = watched();

    w.pty.emit("before");
    let (_, new_ref) = client();
    w.h.attach(&new_ref);

    assert_eq!(w.watching.output(), "before");
}

#[test]
fn sends_the_output_that_comes_during_a_snapshot_after_it_then_replayed() {
    let w = watched();
    w.pty.emit("history\r\n");
    w.h.settled("T-0001", "1");

    let (later, later_ref) = client();
    w.h.attach(&later_ref);
    w.pty.emit("during");
    w.h.settled("T-0001", "1");

    // Rust: the snapshot is cut inside attach, so "during" is not in it: it comes live, once, after
    // replayed, and replayed counts only what the snapshot holds.
    let frames = later.frames();
    assert_eq!(bytes_of(&frames[frames.len() - 1..]), b"during");
    assert_eq!(
        later.controls().last(),
        Some(&ServerMessage::Replayed {
            offset: "history\r\n".len() as u64
        })
    );
    assert_eq!(later.output().matches("during").count(), 1);
    assert_eq!(later.screen(SIZE), ["history", "during"]);
}
