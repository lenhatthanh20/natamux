//! `describe('onOutput')`.

use std::sync::{Arc, Mutex};

use super::util::{attach_at, attach_one};
use crate::support::{SIZE, SetupOptions, setup_with};

#[test]
fn hears_every_piece_of_output_with_the_terminal_name() {
    let heard = Arc::new(Mutex::new(Vec::<String>::new()));
    let sink = Arc::clone(&heard);
    let h = setup_with(SetupOptions {
        on_output: Some(Arc::new(move |task: &str, terminal: &str, data: &[u8]| {
            let text = String::from_utf8_lossy(data);
            sink.lock()
                .unwrap()
                .push(format!("{task}/{terminal}:{text}"));
        })),
        ..SetupOptions::default()
    });
    attach_at(&h, "T-0001", SIZE, "2");

    h.pty(0).emit("hi");

    assert_eq!(*heard.lock().unwrap(), ["T-0001/2:hi"]);
}

#[test]
fn holds_the_start_of_a_character_that_a_read_cut_until_the_rest_comes() {
    let heard = Arc::new(Mutex::new(Vec::<Vec<u8>>::new()));
    let sink = Arc::clone(&heard);
    let h = setup_with(SetupOptions {
        on_output: Some(Arc::new(move |_: &str, _: &str, data: &[u8]| {
            sink.lock().unwrap().push(data.to_vec());
        })),
        ..SetupOptions::default()
    });
    let (watching, _, _) = attach_one(&h);
    h.settled("T-0001", "1");
    let text = "Tiếng Việt 😀 中文\r\n";
    let bytes = text.as_bytes();
    // Inside "ế" (3 bytes) and inside the emoji (4 bytes).
    let cut_a = find(bytes, "ế") + 1;
    let cut_b = find(bytes, "😀") + 2;

    h.pty(0).emit(&bytes[..cut_a]);
    h.pty(0).emit(&bytes[cut_a..cut_b]);
    h.pty(0).emit(&bytes[cut_b..]);
    h.settled("T-0001", "1");

    let mut pieces: Vec<Vec<u8>> = heard.lock().unwrap().clone();
    let heard_all: Vec<u8> = pieces.iter().flatten().copied().collect();
    for frame in watching.frames() {
        if let natamux_daemon::terminals::types::Frame::Bytes(piece) = frame {
            pieces.push(piece.to_vec());
        }
    }
    for piece in &pieces {
        assert!(std::str::from_utf8(piece).is_ok(), "{piece:?}");
    }
    assert_eq!(String::from_utf8(heard_all).unwrap(), text);
    assert!(watching.output().contains(text));
    let (later, _, _) = attach_one(&h);
    h.settled("T-0001", "1");
    assert_eq!(later.screen(SIZE), ["Tiếng Việt 😀 中文"]);
}

fn find(bytes: &[u8], needle: &str) -> usize {
    bytes
        .windows(needle.len())
        .position(|window| window == needle.as_bytes())
        .unwrap()
}
