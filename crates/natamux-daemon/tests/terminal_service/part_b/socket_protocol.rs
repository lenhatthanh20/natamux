//! `describe('the socket protocol')`.

use natamux_daemon::terminals::types::Frame;
use natamux_proto::terminals::messages::ServerMessage;

use super::helpers::drawn;
use crate::support::{SIZE, Size, client, hello_id, is_hello, numbered, numbered_lines, setup};

#[test]
fn says_hello_first_with_one_shell_id_per_shell() {
    let h = setup();
    let (first, first_ref) = client();
    let (second, second_ref) = client();
    h.attach(&first_ref);
    h.pty(0).emit("output");
    h.attach(&second_ref);

    let first_hello = first.controls()[0].clone();
    assert!(is_hello(&first_hello));
    assert_eq!(second.controls()[0], first_hello);

    h.pty(0).exit(Some(0));
    let (fresh, fresh_ref) = client();
    h.attach(&fresh_ref);
    let fresh_hello = fresh.controls()[0].clone();
    assert!(is_hello(&fresh_hello));
    assert_ne!(hello_id(&fresh_hello), hello_id(&first_hello));
}

#[test]
fn sends_the_snapshot_at_the_shell_size_its_bytes_then_replayed_with_the_offset() {
    let screen = Size {
        cols: 100,
        rows: 30,
    };
    let h = setup();
    let (_, first) = client();
    h.attach_to("T-0001", &first, screen, "1", Default::default());
    let printed = [
        "héllo 😀\r\n",
        "Tiếng Việt 中文\r\n",
        "\x1b[31mred\x1b[0m\r\n",
    ];
    for text in printed {
        h.pty(0).emit(text);
    }
    h.settled("T-0001", "1");

    let (later, later_ref) = client();
    h.attach_to("T-0001", &later_ref, screen, "1", Default::default());
    h.settled("T-0001", "1");

    let offset = printed.concat().len() as u64;
    let controls = later.controls();
    assert_eq!(controls.len(), 3);
    assert!(is_hello(&controls[0]));
    assert_eq!(
        controls[1],
        ServerMessage::Snapshot {
            cols: 100,
            rows: 30
        }
    );
    assert_eq!(controls[2], ServerMessage::Replayed { offset });
    let frames = later.frames();
    let chunks = &frames[2..frames.len() - 1];
    assert!(!chunks.is_empty());
    assert!(chunks.iter().all(|frame| matches!(frame, Frame::Bytes(_))));
    assert_eq!(later.screen(screen), ["héllo 😀", "Tiếng Việt 中文", "red"]);

    // Live output after it is binary, and the next client's offset counts its bytes.
    let caught_up = later.frames().len();
    h.pty(0).emit("après");
    h.settled("T-0001", "1");
    let live = &later.frames()[caught_up..];
    assert!(live.iter().all(|frame| matches!(frame, Frame::Bytes(_))));
    assert_eq!(crate::support::bytes_of(live), "après".as_bytes());
    let (third, third_ref) = client();
    h.attach_to("T-0001", &third_ref, screen, "1", Default::default());
    h.settled("T-0001", "1");
    assert_eq!(
        third.controls().last(),
        Some(&ServerMessage::Replayed {
            offset: offset + "après".len() as u64
        })
    );
}

#[test]
fn counts_the_output_that_comes_during_a_snapshot_in_the_offset_of_replayed() {
    let h = setup();
    let (_, first) = client();
    h.attach(&first);
    h.pty(0).emit(numbered(500));
    h.settled("T-0001", "1");

    let (later, later_ref) = client();
    h.attach(&later_ref);
    h.pty(0).emit("during\r\n");
    h.settled("T-0001", "1");

    // Rust: the snapshot is cut inside attach, so "during" is live output after replayed. Its
    // offset still counts it for the next client.
    assert_eq!(
        later.controls().last(),
        Some(&ServerMessage::Replayed {
            offset: numbered(500).len() as u64
        })
    );
    let mut expected = numbered_lines(500);
    expected.push("during".into());
    assert_eq!(later.screen(SIZE), expected);
    let (next, next_ref) = client();
    h.attach(&next_ref);
    let all = format!("{}during\r\n", numbered(500));
    assert_eq!(
        next.controls().last(),
        Some(&ServerMessage::Replayed {
            offset: all.len() as u64
        })
    );
    assert_eq!(drawn(&next.bytes(), SIZE).0, drawn(all.as_bytes(), SIZE).0);
}

#[test]
fn sends_output_as_its_raw_bytes_and_never_an_empty_frame() {
    let h = setup();
    let (watching, watching_ref) = client();
    h.attach(&watching_ref);
    h.settled("T-0001", "1");
    let redraws: Vec<String> = (0..50)
        .map(|step| {
            format!(
                "\x1b[?2026h\x1b[H\x1b[38;5;{step}m{}\x1b[0m\x1b[K\x1b[?2026l",
                "z".repeat(70)
            )
        })
        .collect();
    let start = watching.frames().len();

    for redraw in &redraws {
        h.pty(0).emit(redraw);
        h.pty(0).emit("");
    }
    h.settled("T-0001", "1");

    let live = watching.frames()[start..].to_vec();
    for frame in &live {
        match frame {
            Frame::Bytes(bytes) => assert!(!bytes.is_empty()),
            Frame::Message(message) => panic!("unexpected {message:?}"),
        }
    }
    assert_eq!(crate::support::bytes_of(&live), redraws.concat().as_bytes());
}
