//! `describe('the output ring')`.

use natamux_daemon::terminals::limits::{OUTPUT_FRAME_BYTES, OUTPUT_RING_BYTES};

use crate::support::{client, setup};

#[test]
fn keeps_the_last_frames_and_gives_the_output_since_an_offset_at_a_frame_border() {
    let h = setup();
    let (_, first) = client();
    h.attach(&first);
    let frames = (2 * OUTPUT_RING_BYTES) / OUTPUT_FRAME_BYTES;
    for _ in 0..frames {
        h.pty(0).emit("r".repeat(OUTPUT_FRAME_BYTES));
    }
    let end = (2 * OUTPUT_RING_BYTES) as u64;
    let start = end - OUTPUT_RING_BYTES as u64;
    let frame = OUTPUT_FRAME_BYTES as u64;
    let since = |offset: u64| h.service.output_since("T-0001", offset, "1");

    let kept = since(start).unwrap();
    assert_eq!(kept.len(), OUTPUT_RING_BYTES / OUTPUT_FRAME_BYTES);
    let bytes: Vec<u8> = kept
        .iter()
        .flat_map(|frame| frame.iter().copied())
        .collect();
    assert_eq!(bytes, "r".repeat(OUTPUT_RING_BYTES).as_bytes());
    assert_eq!(
        since(start + frame).unwrap().len(),
        OUTPUT_RING_BYTES / OUTPUT_FRAME_BYTES - 1
    );
    assert_eq!(since(end).unwrap().len(), 0);
    assert!(since(start - frame).is_none());
    assert!(since(end + 1).is_none());
    assert!(since(start + 10).is_none());
    assert!(h.service.output_since("T-0002", 0, "1").is_none());
}
