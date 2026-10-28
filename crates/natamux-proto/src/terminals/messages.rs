//! Messages on the WebSocket of a task terminal, as in `packages/core/src/terminals/terminal.ts`.
//! Output comes in binary frames: the raw bytes for the screen. Every other message is one JSON
//! object in a text frame.

use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Close codes of the terminal socket. After any of them a client does not reconnect. GONE: the
/// shell of a resume no longer runs.
pub const CLOSE_UNAUTHORIZED: u16 = 4401;
pub const CLOSE_REFUSED: u16 = 4409;
pub const CLOSE_GONE: u16 = 4410;
pub const FINAL_CLOSE_CODES: [u16; 3] = [CLOSE_UNAUTHORIZED, CLOSE_REFUSED, CLOSE_GONE];

/// The client acks the output it has parsed after this many bytes, and once after a catch-up.
pub const ACK_EVERY_BYTES: u64 = 64 * 1024;

/// The client pings this often, and reconnects when no pong came for two of these.
pub const HEARTBEAT: Duration = Duration::from_secs(15);

/// A lost socket is tried again after min(RECONNECT_MAX, RECONNECT_BASE * 2^try), give or take
/// RECONNECT_JITTER of it.
pub const RECONNECT_BASE: Duration = Duration::from_millis(500);
pub const RECONNECT_MAX: Duration = Duration::from_secs(10);
pub const RECONNECT_JITTER: f64 = 0.2;

/// The daemon drops a socket that sent nothing for this long (`SILENT_CLOSE_MS`): a page in the
/// background still pings, a laptop that slept does not.
pub const SILENT_CLOSE: Duration = Duration::from_secs(90);

/// A catch-up whose `replayed` never comes ends after this, so a daemon bug never leaves a blank
/// pane.
pub const CATCH_UP_TIMEOUT: Duration = Duration::from_secs(10);

/// The daemon takes at most this many UTF-16 code units in one `input` message (zod counts a
/// JavaScript string's length).
pub const MAX_INPUT_UNITS: usize = 64 * 1024;

/// Columns and rows are 1 to this.
pub const MAX_SIZE: u16 = 1000;

/// The agent of a task runs in its first terminal.
pub const MAIN_TERMINAL: &str = "1";

/// From the client to the daemon.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ClientMessage {
    Input {
        data: String,
    },
    Resize {
        cols: u16,
        rows: u16,
    },
    /// `cols` and `rows` come only when the terminal itself came on screen; only then it claims
    /// the shell's size. `park` with `visible: false`: the daemon sends this client no live output
    /// until it is visible again, and then catches it up at once. Left out when false, so the web
    /// and old clients send the same JSON as before.
    Visible {
        visible: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        cols: Option<u16>,
        #[serde(skip_serializing_if = "Option::is_none")]
        rows: Option<u16>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        park: bool,
    },
    Ack {
        offset: u64,
    },
    Ping,
}

impl ClientMessage {
    pub fn to_json(&self) -> String {
        // A plain enum of strings and numbers always serializes.
        serde_json::to_string(self).unwrap_or_default()
    }

    /// The daemon's side: the rules of `terminalClientMessageSchema` that the JSON shape does not
    /// hold. A message that breaks one is ignored, as Node ignores it.
    pub fn is_valid(&self) -> bool {
        let size = |value: u16| (1..=MAX_SIZE).contains(&value);
        match self {
            ClientMessage::Input { data } => data.encode_utf16().count() <= MAX_INPUT_UNITS,
            ClientMessage::Resize { cols, rows } => size(*cols) && size(*rows),
            ClientMessage::Visible { cols, rows, .. } => {
                cols.is_none_or(size) && rows.is_none_or(size)
            }
            ClientMessage::Ack { .. } | ClientMessage::Ping => true,
        }
    }
}

/// From the daemon to the client. `hello` is always first. `snapshot` and `resume` start a
/// catch-up and `replayed` ends it. `pong` answers `ping`. `exit` ends the session; `paused` ends
/// an idle agent's shell, which starts again on the next fresh attach; `error` says why it could
/// not start.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ServerMessage {
    Hello {
        #[serde(rename = "shellId")]
        shell_id: String,
    },
    Snapshot {
        cols: u16,
        rows: u16,
    },
    Resume {
        offset: u64,
    },
    Replayed {
        offset: u64,
    },
    Pong,
    Exit {
        code: Option<i32>,
    },
    /// The agent sat idle this many minutes with no one looking.
    Paused {
        minutes: u64,
    },
    Error {
        message: String,
    },
}

impl ServerMessage {
    /// None for anything that is not one of the messages above, as the web client ignores it.
    pub fn parse(text: &str) -> Option<ServerMessage> {
        serde_json::from_str(text).ok()
    }

    /// The daemon's side: one JSON text frame, keys in the order Node writes them.
    pub fn to_json(&self) -> String {
        // A plain enum of strings and numbers always serializes.
        serde_json::to_string(self).unwrap_or_default()
    }
}

/// Splits typed or pasted text into `input` messages the daemon takes, never inside a character.
pub fn input_chunks(text: &str) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut chunk = String::new();
    let mut units = 0;
    for character in text.chars() {
        let width = character.len_utf16();
        if units + width > MAX_INPUT_UNITS {
            chunks.push(std::mem::take(&mut chunk));
            units = 0;
        }
        chunk.push(character);
        units += width;
    }
    if !chunk.is_empty() {
        chunks.push(chunk);
    }
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    // JSON as the TypeScript client and daemon write it (terminal.ts in @natamux/core).
    #[test]
    fn client_messages_match_the_typescript_schema() {
        let cases = [
            (
                ClientMessage::Input {
                    data: "ls\r".into(),
                },
                r#"{"type":"input","data":"ls\r"}"#,
            ),
            (
                ClientMessage::Resize {
                    cols: 120,
                    rows: 40,
                },
                r#"{"type":"resize","cols":120,"rows":40}"#,
            ),
            (
                ClientMessage::Visible {
                    visible: true,
                    cols: Some(80),
                    rows: Some(24),
                    park: false,
                },
                r#"{"type":"visible","visible":true,"cols":80,"rows":24}"#,
            ),
            (
                ClientMessage::Visible {
                    visible: false,
                    cols: None,
                    rows: None,
                    park: false,
                },
                r#"{"type":"visible","visible":false}"#,
            ),
            (
                ClientMessage::Visible {
                    visible: false,
                    cols: None,
                    rows: None,
                    park: true,
                },
                r#"{"type":"visible","visible":false,"park":true}"#,
            ),
            (
                ClientMessage::Ack { offset: 65536 },
                r#"{"type":"ack","offset":65536}"#,
            ),
            (ClientMessage::Ping, r#"{"type":"ping"}"#),
        ];
        for (message, json) in cases {
            assert_eq!(message.to_json(), json);
            assert_eq!(
                serde_json::from_str::<ClientMessage>(json).unwrap(),
                message
            );
        }
    }

    // The web never sends `park`: its JSON must not change, and a missing `park` is false.
    #[test]
    fn park_is_left_out_when_false_and_read_as_false_when_missing() {
        let hidden = ClientMessage::Visible {
            visible: false,
            cols: None,
            rows: None,
            park: false,
        };
        assert!(!hidden.to_json().contains("park"));
        let missing = r#"{"type":"visible","visible":false}"#;
        assert_eq!(
            serde_json::from_str::<ClientMessage>(missing).unwrap(),
            hidden
        );
        let parked = ClientMessage::Visible {
            visible: false,
            cols: None,
            rows: None,
            park: true,
        };
        let json = parked.to_json();
        assert_eq!(
            serde_json::from_str::<ClientMessage>(&json).unwrap(),
            parked
        );
    }

    #[test]
    fn server_messages_match_the_typescript_schema() {
        let cases = [
            (
                r#"{"type":"hello","shellId":"q2V3-x_9"}"#,
                ServerMessage::Hello {
                    shell_id: "q2V3-x_9".into(),
                },
            ),
            (
                r#"{"type":"snapshot","cols":100,"rows":30}"#,
                ServerMessage::Snapshot {
                    cols: 100,
                    rows: 30,
                },
            ),
            (
                r#"{"type":"resume","offset":12}"#,
                ServerMessage::Resume { offset: 12 },
            ),
            (
                r#"{"type":"replayed","offset":4096}"#,
                ServerMessage::Replayed { offset: 4096 },
            ),
            (r#"{"type":"pong"}"#, ServerMessage::Pong),
            (
                r#"{"type":"exit","code":0}"#,
                ServerMessage::Exit { code: Some(0) },
            ),
            (
                r#"{"type":"exit","code":null}"#,
                ServerMessage::Exit { code: None },
            ),
            (
                r#"{"type":"paused","minutes":30}"#,
                ServerMessage::Paused { minutes: 30 },
            ),
            (
                r#"{"type":"error","message":"Too many terminals."}"#,
                ServerMessage::Error {
                    message: "Too many terminals.".into(),
                },
            ),
        ];
        for (json, message) in cases {
            assert_eq!(ServerMessage::parse(json), Some(message.clone()));
            assert_eq!(
                ServerMessage::parse(&serde_json::to_string(&message).unwrap()),
                Some(message)
            );
        }
    }

    #[test]
    fn the_daemon_checks_sizes_and_input_length() {
        let valid = |json: &str| {
            serde_json::from_str::<ClientMessage>(json)
                .unwrap()
                .is_valid()
        };
        assert!(valid(r#"{"type":"resize","cols":1000,"rows":1}"#));
        assert!(!valid(r#"{"type":"resize","cols":0,"rows":24}"#));
        assert!(!valid(r#"{"type":"visible","visible":true,"cols":1001}"#));
        assert!(valid(r#"{"type":"visible","visible":false}"#));
        assert!(valid(r#"{"type":"ack","offset":0}"#));
        let long = ClientMessage::Input {
            data: "a".repeat(MAX_INPUT_UNITS + 1),
        };
        assert!(!long.is_valid());
        assert_eq!(
            ServerMessage::Exit { code: None }.to_json(),
            r#"{"type":"exit","code":null}"#
        );
    }

    #[test]
    fn ignores_what_it_does_not_know() {
        assert_eq!(ServerMessage::parse(r#"{"type":"later"}"#), None);
        assert_eq!(ServerMessage::parse("not json"), None);
        assert_eq!(ServerMessage::parse(r#"{"type":"resume"}"#), None);
    }

    #[test]
    fn splits_input_by_utf16_units_and_never_inside_a_character() {
        assert!(input_chunks("").is_empty());
        assert_eq!(input_chunks("người"), ["người"]);

        let long = "a".repeat(MAX_INPUT_UNITS + 5);
        let chunks = input_chunks(&long);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].len(), MAX_INPUT_UNITS);
        assert_eq!(chunks[1].len(), 5);

        // An emoji is two UTF-16 units: it may not straddle the limit.
        let mut edge = "a".repeat(MAX_INPUT_UNITS - 1);
        edge.push('😀');
        let chunks = input_chunks(&edge);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[1], "😀");
        assert_eq!(chunks.concat(), edge);
    }
}
