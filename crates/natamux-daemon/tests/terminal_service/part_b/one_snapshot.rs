//! `describe('one snapshot for several clients')`. Rust cuts a snapshot inside `attach`, so
//! clients share one only while a synchronized frame makes it wait. Counting serialize calls is
//! left out: no spy, no such mechanism.

use crate::support::{Harness, SIZE, Size, client, setup};

use super::helpers::kinds;
use natamux_proto::terminals::messages::ServerMessage;

const WIDE: Size = Size {
    cols: 100,
    rows: 30,
};

/// A shell that printed "quiet" and is inside a synchronized frame.
fn drawing() -> Harness {
    let h = setup();
    let (_, first) = client();
    h.attach(&first);
    h.pty(0).emit("quiet\r\n\x1b[?2026h");
    h.settled("T-0001", "1");
    h
}

#[test]
fn serializes_once_for_clients_that_attach_together_even_if_one_leaves() {
    let h = drawing();
    let clients: Vec<_> = (0..4).map(|_| client()).collect();
    let detaches: Vec<_> = clients.iter().map(|(_, each)| h.attach(each)).collect();
    detaches[3].detach();
    h.pty(0).emit("\x1b[?2026l");
    h.settled("T-0001", "1");

    let first = clients[0].0.frames();
    assert_eq!(clients[0].0.screen(SIZE), ["quiet"]);
    assert_eq!(clients[1].0.frames(), first);
    assert_eq!(clients[2].0.frames(), first);
    assert_eq!(kinds(&clients[3].0), ["hello"]);
}

#[test]
fn lets_late_clients_share_the_next_snapshot_and_gives_a_joiner_each_byte_once() {
    let h = drawing();
    let (first, first_ref) = client();
    let (joiner, joiner_ref) = client();
    h.attach(&first_ref);
    h.attach(&joiner_ref);
    h.pty(0).emit("more\r\n\x1b[?2026l");
    h.settled("T-0001", "1");
    let (after_output, after_ref) = client();
    let (other_size, other_ref) = client();
    h.attach(&after_ref);
    h.attach_to("T-0001", &other_ref, WIDE, "1", Default::default());
    h.settled("T-0001", "1");

    for each in [&first, &joiner] {
        assert_eq!(each.screen(SIZE), ["quiet", "more"]);
        assert_eq!(each.output().matches("more").count(), 1);
    }
    assert_eq!(first.frames(), joiner.frames());
    assert_eq!(after_output.screen(SIZE), ["quiet", "more"]);
    assert!(other_size.controls().contains(&ServerMessage::Snapshot {
        cols: 100,
        rows: 30
    }));
    assert_eq!(other_size.screen(WIDE), ["quiet", "more"]);
}
