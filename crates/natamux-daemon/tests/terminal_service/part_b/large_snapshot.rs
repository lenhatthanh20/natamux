//! `describe('sending a large snapshot')`. Rust sends a snapshot inside `attach`, not over several
//! turns: what holds is that no frame is larger than a frame and that the client ends on the
//! right screen, with the output that came after it once.

use natamux_daemon::terminals::limits::OUTPUT_FRAME_BYTES;
use natamux_daemon::terminals::types::Frame;
use natamux_proto::terminals::messages::ServerMessage;

use super::helpers::assert_same_screen;
use crate::support::{SIZE, client, is_hello, setup};

#[test]
fn sends_it_in_frames_of_at_most_one_frame_with_the_output_that_came_after_it() {
    let h = setup();
    let (_, first) = client();
    h.attach(&first).detach();
    let history = format!("{}\r\n", "s".repeat(78)).repeat(8000);
    h.pty(0).emit(&history);
    h.settled("T-0001", "1");

    let (later, later_ref) = client();
    let (joiner, joiner_ref) = client();
    h.attach(&later_ref);
    h.attach(&joiner_ref);
    h.pty(0).emit("during\r\n");
    h.settled("T-0001", "1");

    let controls = later.controls();
    assert_eq!(controls.len(), 3);
    assert!(is_hello(&controls[0]));
    assert_eq!(controls[1], ServerMessage::Snapshot { cols: 80, rows: 24 });
    assert_eq!(
        controls[2],
        ServerMessage::Replayed {
            offset: history.len() as u64
        }
    );
    let mut frames = 0;
    for frame in later.frames() {
        if let Frame::Bytes(bytes) = frame {
            assert!(bytes.len() <= OUTPUT_FRAME_BYTES);
            frames += 1;
        }
    }
    // The snapshot is more than one frame, and "during" is the last one.
    assert!(frames > 2);
    assert_eq!(&later.bytes()[later.bytes().len() - 8..], b"during\r\n");
    assert_eq!(later.output().matches("during").count(), 1);
    assert_eq!(joiner.bytes(), later.bytes());
    assert_same_screen(&later, &format!("{history}during\r\n"), SIZE);
}
