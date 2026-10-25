//! `describe('a client that falls behind')`.

use std::sync::Arc;

use natamux_daemon::terminals::limits::{
    CLIENT_HIGH_WATER, OUTPUT_FRAME_BYTES, RESYNC_QUIET_WINDOW_MS,
};
use natamux_daemon::terminals::shells::Detach;
use natamux_daemon::terminals::types::{ClientRef, Frame};

use super::util::{MAIN, TASK, Watcher, adv, catch_ups, frame_of, frames_from};
use crate::support::{FakePty, Harness, Recorder, bytes_of, client, setup};

const FRAME: u64 = OUTPUT_FRAME_BYTES as u64;

struct Shell {
    h: Harness,
    pty: FakePty,
    slow: Arc<Recorder>,
    slow_ref: ClientRef,
    detach: Detach,
}

fn shell() -> Shell {
    let h = setup();
    let (slow, slow_ref) = client();
    let detach = h.attach(&slow_ref);
    h.settled(TASK, MAIN);
    let pty = h.pty(0);
    Shell {
        h,
        pty,
        slow,
        slow_ref,
        detach,
    }
}

/// Frames from `from` on, a few at a time, so the batch is sent now and then.
fn print(h: &Harness, pty: &FakePty, watchers: &[&Watcher], from: usize, count: usize) {
    for index in from..from + count {
        pty.emit(frame_of(index));
        if index % 4 == 3 {
            h.settled(TASK, MAIN);
            watchers.iter().for_each(|watcher| watcher.deliver(h));
        }
    }
    h.settled(TASK, MAIN);
    watchers.iter().for_each(|watcher| watcher.deliver(h));
}

/// The live bytes a client got, up to a catch-up.
fn live_bytes(received: &[Frame]) -> u64 {
    received
        .iter()
        .map_while(|frame| match frame {
            Frame::Bytes(bytes) => Some(bytes.len() as u64),
            Frame::Message(_) => None,
        })
        .sum()
}

#[test]
fn drops_the_live_output_of_a_client_past_client_high_water_unacked_and_only_of_that_one() {
    let Shell { h, pty, slow, .. } = shell();
    let (fast, _detach) = Watcher::attach(&h, MAIN, true);
    h.settled(TASK, MAIN);
    let start = (slow.frames().len(), fast.frames().len());
    let fast_ws = [&fast];

    print(&h, &pty, &fast_ws, 0, (CLIENT_HIGH_WATER / FRAME) as usize);
    assert_eq!(live_bytes(&slow.frames()[start.0..]), CLIENT_HIGH_WATER);
    pty.emit("!");
    print(&h, &pty, &fast_ws, 8, 48);

    // Nothing more for the slow one: no queue grows for it.
    assert_eq!(live_bytes(&slow.frames()[start.0..]), CLIENT_HIGH_WATER);
    let fast_output = String::from_utf8_lossy(&bytes_of(&fast.frames()[start.1..])).into_owned();
    assert_eq!(
        fast_output,
        format!("{}!{}", frames_from(0, 8), frames_from(8, 48))
    );
    assert_eq!(catch_ups(&fast.frames()), []);
    assert!(pty.state().flow.is_empty());
}

#[test]
fn does_not_count_a_snapshot_or_the_output_that_waited_as_unacked() {
    let h = setup();
    let (_, first) = client();
    h.attach(&first).detach();
    let pty = h.pty(0);
    let history = format!("{}\r\n", "s".repeat(78)).repeat(8000);
    pty.emit(&history);
    h.settled(TASK, MAIN);

    let (later, later_ref) = client();
    h.attach(&later_ref);
    pty.emit("during\r\n");
    h.settled(TASK, MAIN);
    let offset = (history.len() + "during\r\n".len()) as u64;
    assert!(bytes_of(&later.frames()).len() as u64 > CLIENT_HIGH_WATER);
    // Rust: the snapshot is sent inside attach, so its replayed comes before "during".
    h.service.ack(TASK, MAIN, &later_ref, offset);
    let caught_up = later.frames().len();

    print(&h, &pty, &[], 0, 8);
    pty.emit("!");
    h.settled(TASK, MAIN);

    assert_eq!(
        bytes_of(&later.frames()[caught_up..]).len() as u64,
        CLIENT_HIGH_WATER
    );
}

#[test]
fn takes_an_ack_past_what_was_sent_as_all_of_it_and_ignores_an_older_one() {
    let Shell {
        h,
        pty,
        slow,
        slow_ref,
        ..
    } = shell();
    let start = slow.frames().len();

    h.service.ack(TASK, MAIN, &slow_ref, 10 * CLIENT_HIGH_WATER);
    print(&h, &pty, &[], 0, 4);
    h.service.ack(TASK, MAIN, &slow_ref, 4 * FRAME);
    h.service.ack(TASK, MAIN, &slow_ref, FRAME);
    print(&h, &pty, &[], 4, 9);

    // 8 frames past the ack fit CLIENT_HIGH_WATER; the 9th does not.
    assert_eq!(bytes_of(&slow.frames()[start..]).len() as u64, 12 * FRAME);
}

#[test]
fn forgets_a_client_that_leaves_while_behind() {
    let Shell {
        h,
        pty,
        slow,
        slow_ref,
        detach,
    } = shell();
    print(&h, &pty, &[], 0, 9);
    h.service.ack(TASK, MAIN, &slow_ref, CLIENT_HIGH_WATER);

    detach.detach();
    let count = slow.frames().len();
    pty.emit("after\r\n");
    h.settled(TASK, MAIN);
    adv(&h, &[], 2 * RESYNC_QUIET_WINDOW_MS);

    assert_eq!(slow.frames().len(), count);
}
