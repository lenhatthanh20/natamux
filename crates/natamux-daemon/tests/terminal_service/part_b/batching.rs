//! `describe('batching output')`. `h.tick()` is the setImmediate turn of the Node tests.

use std::sync::{Arc, Mutex};

use natamux_daemon::terminals::limits::{
    OUTPUT_COALESCE_MS, OUTPUT_FRAME_BYTES, OUTPUT_IDLE_BYTES,
};
use natamux_daemon::terminals::types::Frame;
use natamux_proto::terminals::messages::ServerMessage;

use crate::support::{FakePty, Harness, Recorder, SetupOptions, bytes_of, client, setup_with};

fn watched(options: SetupOptions) -> (Harness, FakePty, Arc<Recorder>) {
    let h = setup_with(options);
    let (watching, watching_ref) = client();
    h.attach(&watching_ref);
    h.tick();
    assert!(matches!(
        watching.controls().last(),
        Some(ServerMessage::Replayed { .. })
    ));
    let pty = h.pty(0);
    (h, pty, watching)
}

fn sent(watching: &Recorder, start: usize) -> Vec<Frame> {
    watching.frames()[start..].to_vec()
}

#[test]
fn sends_a_burst_of_chunks_in_few_frames_in_order_with_all_bytes() {
    let heard: Arc<Mutex<Vec<Vec<u8>>>> = Arc::default();
    let sink = Arc::clone(&heard);
    let (h, pty, watching) = watched(SetupOptions {
        on_output: Some(Arc::new(move |_, _, data| {
            sink.lock().unwrap().push(data.to_vec())
        })),
        ..SetupOptions::default()
    });
    let chunks: Vec<String> = (0..1000)
        .map(|index| format!("chunk {index} {}\r\n", "x".repeat(100)))
        .collect();
    let start = watching.frames().len();

    for chunk in &chunks {
        pty.emit(chunk);
    }
    h.tick();

    let frames = sent(&watching, start);
    let total = chunks.concat().len();
    assert!(frames.iter().all(|frame| matches!(frame, Frame::Bytes(_))));
    assert!(frames.len() <= total.div_ceil(OUTPUT_FRAME_BYTES));
    assert_eq!(bytes_of(&frames), chunks.concat().as_bytes());
    let heard: Vec<Vec<u8>> = heard.lock().unwrap().clone();
    assert_eq!(
        heard,
        chunks
            .iter()
            .map(|chunk| chunk.as_bytes().to_vec())
            .collect::<Vec<_>>()
    );
}

#[test]
fn sends_a_full_frame_at_once_and_starts_the_next_frame_with_the_next_byte() {
    let (h, pty, watching) = watched(SetupOptions::default());
    let start = watching.frames().len();
    let sizes = || -> Vec<usize> {
        sent(&watching, start)
            .iter()
            .filter_map(|frame| match frame {
                Frame::Bytes(bytes) => Some(bytes.len()),
                Frame::Message(_) => None,
            })
            .collect()
    };

    pty.emit("x".repeat(OUTPUT_FRAME_BYTES));
    assert_eq!(sizes(), [OUTPUT_FRAME_BYTES]);

    pty.emit("y");
    h.tick();
    assert_eq!(sizes().len(), 1);
    h.advance(OUTPUT_COALESCE_MS);
    assert_eq!(sizes(), [OUTPUT_FRAME_BYTES, 1]);
}

#[test]
fn splits_a_chunk_larger_than_a_frame_at_character_borders() {
    let (h, pty, watching) = watched(SetupOptions::default());
    let start = watching.frames().len();
    let text = "aé中😀".repeat(20_000);

    pty.emit(&text);
    h.advance(OUTPUT_COALESCE_MS);

    let frames = sent(&watching, start);
    assert!(frames.len() > 3);
    let mut joined = String::new();
    for frame in &frames {
        let Frame::Bytes(bytes) = frame else {
            panic!("not bytes");
        };
        assert!(bytes.len() <= OUTPUT_FRAME_BYTES);
        joined.push_str(std::str::from_utf8(bytes).expect("a frame ends inside a character"));
    }
    assert_eq!(joined, text);
}

#[test]
fn sends_a_lone_small_chunk_on_the_next_turn_with_no_delay() {
    let (h, pty, watching) = watched(SetupOptions::default());

    pty.emit("a");
    assert_eq!(watching.output(), "");
    h.tick();
    assert_eq!(watching.output(), "a");

    h.advance(OUTPUT_COALESCE_MS);
    pty.emit("b");
    h.tick();
    assert_eq!(watching.output(), "ab");
}

#[test]
fn waits_for_more_output_when_a_chunk_is_not_small() {
    let (h, pty, watching) = watched(SetupOptions::default());

    pty.emit("x".repeat(OUTPUT_IDLE_BYTES));
    h.tick();
    assert_eq!(watching.output(), "");
    h.advance(OUTPUT_COALESCE_MS);
    assert_eq!(watching.output(), "x".repeat(OUTPUT_IDLE_BYTES));
}

#[test]
fn waits_for_more_output_when_the_last_frame_went_out_just_now() {
    let (h, pty, watching) = watched(SetupOptions::default());
    pty.emit("a");
    h.tick();

    pty.emit("b");
    h.tick();
    assert_eq!(watching.output(), "a");
    h.advance(OUTPUT_COALESCE_MS);
    assert_eq!(watching.output(), "ab");

    h.advance(OUTPUT_COALESCE_MS - 1);
    pty.emit("c");
    h.tick();
    assert_eq!(watching.output(), "ab");
    h.advance(OUTPUT_COALESCE_MS);
    assert_eq!(watching.output(), "abc");
}
