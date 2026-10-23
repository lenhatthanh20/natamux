//! Small helpers of part B.

use natamux_proto::terminals::messages::ServerMessage;
use natamux_screen::Screen;

use crate::support::{Recorder, Size};

/// Plain text, colors, wide characters, an emoji at the last column and one that does not fit
/// there, Vietnamese, and lines longer than the screen, colored across the wrap.
pub fn mixed_lines(cols: usize, count: usize) -> String {
    let mut text = String::new();
    for index in 0..count {
        let line = match index % 7 {
            0 => format!("plain line {index}"),
            1 => format!(
                "\x1b[1;31mbold red {index}\x1b[0m, \x1b[4;38;5;33munder\x1b[24m, \x1b[3;44mblue\x1b[0m end"
            ),
            2 => format!("wide 中文字 {index}"),
            3 => format!("{}😀", "e".repeat(cols - 2)),
            4 => format!("{}😀 next row", "f".repeat(cols - 1)),
            5 => format!(
                "\x1b[32m{}\x1b[0m{} {index}",
                "g".repeat(cols + 3),
                "w".repeat(cols)
            ),
            _ => format!("Tiếng Việt có dấu {index}"),
        };
        text.push_str(&line);
        text.push_str("\r\n");
    }
    text
}

/// What a fresh terminal of `size` draws after `bytes`: its text and its snapshot (cursor and
/// styles included).
pub fn drawn(bytes: &[u8], size: Size) -> (String, Vec<u8>) {
    let mut screen = Screen::new(size.cols, size.rows);
    screen.write(bytes);
    (screen.last_lines(usize::MAX), screen.snapshot())
}

/// The client holds the screen the shell output draws: same text, and the same cursor and styles
/// (the snapshot of a screen holds both).
pub fn assert_same_screen(later: &Recorder, output: &str, size: Size) {
    let expected = drawn(output.as_bytes(), size);
    let actual = drawn(&later.bytes(), size);
    assert_eq!(actual.0, expected.0);
    assert!(
        actual.1 == expected.1,
        "the snapshots of the two screens differ"
    );
}

/// The kinds of control messages a client got, in order.
pub fn kinds(client: &Recorder) -> Vec<&'static str> {
    client
        .controls()
        .iter()
        .map(|message| match message {
            ServerMessage::Hello { .. } => "hello",
            ServerMessage::Snapshot { .. } => "snapshot",
            ServerMessage::Resume { .. } => "resume",
            ServerMessage::Replayed { .. } => "replayed",
            ServerMessage::Exit { .. } => "exit",
            _ => "other",
        })
        .collect()
}
