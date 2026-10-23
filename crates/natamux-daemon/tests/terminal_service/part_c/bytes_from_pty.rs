//! `describe('bytes from the pty')`.

use std::sync::{Arc, Mutex};

use natamux_daemon::terminals::limits::{
    HIDDEN_BUDGET_BYTES, OUTPUT_FRAME_BYTES, RESYNC_MAX_WAIT_MS,
};
use natamux_proto::terminals::messages::ServerMessage;

use super::util::{MAIN, TASK, Watcher, adv, catch_ups, frame_of};
use crate::support::{SIZE, SetupOptions, bytes_of, client, controls, setup, setup_with};

#[test]
fn keeps_an_escape_sequence_that_two_reads_split() {
    let h = setup();
    let (watching, watching_ref) = client();
    h.attach(&watching_ref);
    h.settled(TASK, MAIN);
    let pty = h.pty(0);

    pty.emit("\x1b[3");
    pty.emit("1mred\x1b[0m plain\r\n");
    h.settled(TASK, MAIN);

    assert!(watching.output().contains("\x1b[31mred\x1b[0m plain\r\n"));
    let (later, later_ref) = client();
    h.attach(&later_ref);
    h.settled(TASK, MAIN);
    assert_eq!(later.screen(SIZE), ["red plain"]);
}

#[test]
fn sends_nothing_for_a_read_that_is_only_the_start_of_a_character() {
    let heard: Arc<Mutex<Vec<String>>> = Arc::default();
    let sink = Arc::clone(&heard);
    let h = setup_with(SetupOptions {
        on_output: Some(Arc::new(move |_task, _terminal, data| {
            sink.lock()
                .unwrap()
                .push(String::from_utf8_lossy(data).into_owned());
        })),
        ..SetupOptions::default()
    });
    let (watching, watching_ref) = client();
    h.attach(&watching_ref);
    h.settled(TASK, MAIN);
    let before = watching.bytes().len();
    let bytes = "中".as_bytes();
    let pty = h.pty(0);

    pty.emit(&bytes[..1]);
    h.settled(TASK, MAIN);
    assert!(heard.lock().unwrap().is_empty());
    assert_eq!(watching.bytes().len(), before);

    pty.emit(&bytes[1..]);
    h.settled(TASK, MAIN);
    assert_eq!(*heard.lock().unwrap(), ["中"]);
    assert_eq!(String::from_utf8_lossy(&watching.bytes()[before..]), "中");
}

#[test]
fn keeps_every_line_of_a_flood_while_hidden_through_a_ring_resume_when_shown() {
    let h = setup();
    let (watching, _detach) = Watcher::attach(&h, MAIN, false);
    adv(&h, &[&watching], 0);
    let pty = h.pty(0);
    let mut printed = String::new();

    // Twice the budget of one window: the second half is dropped while hidden.
    for index in 0..(2 * HIDDEN_BUDGET_BYTES) as usize / OUTPUT_FRAME_BYTES {
        pty.emit(frame_of(index));
        printed.push_str(&frame_of(index));
    }
    adv(&h, &[&watching], RESYNC_MAX_WAIT_MS);
    h.service.visible(TASK, MAIN, &watching.client, true, None);
    adv(&h, &[&watching], 0);

    let frames = watching.frames();
    assert_eq!(
        catch_ups(&frames),
        [ServerMessage::Resume {
            offset: HIDDEN_BUDGET_BYTES
        }]
    );
    assert_eq!(
        controls(&frames).last(),
        Some(&ServerMessage::Replayed {
            offset: 2 * HIDDEN_BUDGET_BYTES
        })
    );
    let attached = frames
        .iter()
        .position(|frame| {
            matches!(
                frame,
                natamux_daemon::terminals::types::Frame::Message(ServerMessage::Replayed { .. })
            )
        })
        .unwrap();
    let rest = String::from_utf8_lossy(&bytes_of(&frames[attached + 1..])).into_owned();
    assert_eq!(rest, printed);
}
