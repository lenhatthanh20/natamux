//! Helpers of the Node test file: `frameOf`, `ackingClient`, `catchUps`, `flood`, `sentTo`,
//! `sinceSnapshot`, `expectSameScreen`.

use std::sync::{Arc, Mutex};

use natamux_daemon::terminals::limits::OUTPUT_FRAME_BYTES;
use natamux_daemon::terminals::service::AttachOptions;
use natamux_daemon::terminals::shells::Detach;
use natamux_daemon::terminals::types::{ClientRef, Frame, TerminalClient};
use natamux_proto::terminals::history::CATCH_UP_HISTORY_ROWS;
use natamux_proto::terminals::messages::{ACK_EVERY_BYTES, ServerMessage};

use crate::support::{FakePty, Harness, SIZE, Size, bytes_of, controls, screen_of};

pub const TASK: &str = "T-0001";
pub const MAIN: &str = "1";

/// One full output frame of numbered lines, 64 bytes each.
pub fn frame_of(index: usize) -> String {
    let lines = OUTPUT_FRAME_BYTES / 64;
    (0..lines)
        .map(|line| format!("{:.>62}\r\n", index * lines + line))
        .collect()
}

pub fn frames_from(from: usize, count: usize) -> String {
    (from..from + count).map(frame_of).collect()
}

pub fn snapshot_msg() -> ServerMessage {
    ServerMessage::Snapshot {
        cols: SIZE.cols,
        rows: SIZE.rows,
    }
}

/// The catch-ups a client got after the snapshot of its attach.
pub fn catch_ups(frames: &[Frame]) -> Vec<ServerMessage> {
    controls(frames)
        .into_iter()
        .filter(|message| {
            matches!(
                message,
                ServerMessage::Snapshot { .. } | ServerMessage::Resume { .. }
            )
        })
        .skip(1)
        .collect()
}

fn replayed_at(frame: &Frame) -> Option<u64> {
    match frame {
        Frame::Message(ServerMessage::Replayed { offset }) => Some(*offset),
        _ => None,
    }
}

/// The end offset of the output a client got: its last replayed, then the live bytes after it.
pub fn sent_to(frames: &[Frame]) -> u64 {
    let at = frames
        .iter()
        .rposition(|frame| replayed_at(frame).is_some());
    let base = at.and_then(|at| replayed_at(&frames[at])).unwrap_or(0);
    base + bytes_of(&frames[at.map_or(0, |at| at + 1)..]).len() as u64
}

/// The frames from the last snapshot on: what the client's screen holds after it.
pub fn since_snapshot(frames: &[Frame]) -> Vec<Frame> {
    let at = frames
        .iter()
        .rposition(|frame| matches!(frame, Frame::Message(ServerMessage::Snapshot { .. })));
    frames[at.unwrap_or(0)..].to_vec()
}

/// The client's screen equals the daemon's, as deep as the history a catch-up sends: a client
/// that was caught up holds only the last `CATCH_UP_HISTORY_ROWS` of it (T-0223 P8).
pub fn expect_same_screen(frames: &[Frame], daemon: &[Frame]) {
    let expected = screen_of(daemon, SIZE);
    let got = screen_of(frames, SIZE);
    let floor = expected.len().min(CATCH_UP_HISTORY_ROWS);
    assert!(
        got.len() >= floor,
        "the screen is shorter: {} lines, {} at least",
        got.len(),
        floor
    );
    let deep = expected.len().min(got.len());
    assert_eq!(got[got.len() - deep..], expected[expected.len() - deep..]);
}

/// What a client attached now gets, with its snapshot.
pub fn daemon_screen(h: &Harness, watchers: &[&Watcher]) -> Vec<Frame> {
    let (fresh, fresh_ref) = crate::support::client();
    h.attach(&fresh_ref);
    adv(h, watchers, 1000);
    assert!(matches!(
        controls(&fresh.frames()).last(),
        Some(ServerMessage::Replayed { .. })
    ));
    fresh.frames()
}

#[derive(Default)]
struct AckState {
    offset: u64,
    acked: u64,
    live: bool,
    pending: Vec<u64>,
}

struct AckingClient {
    frames: Mutex<Vec<Frame>>,
    arrivals: Mutex<Vec<(u64, u64)>>,
    state: Mutex<AckState>,
    now: Box<dyn Fn() -> u64 + Send + Sync>,
}

impl TerminalClient for AckingClient {
    fn send(&self, frame: Frame) {
        let mut state = self.state.lock().unwrap();
        match &frame {
            Frame::Message(ServerMessage::Snapshot { .. } | ServerMessage::Resume { .. }) => {
                state.live = false;
            }
            Frame::Message(ServerMessage::Replayed { offset }) => {
                state.live = true;
                state.offset = *offset;
                state.acked = *offset;
                state.pending.push(*offset);
            }
            Frame::Bytes(bytes) if state.live => {
                state.offset += bytes.len() as u64;
                self.arrivals
                    .lock()
                    .unwrap()
                    .push(((self.now)(), bytes.len() as u64));
                if state.offset - state.acked >= ACK_EVERY_BYTES {
                    state.acked = state.offset;
                    let at = state.offset;
                    state.pending.push(at);
                }
            }
            _ => {}
        }
        drop(state);
        self.frames.lock().unwrap().push(frame);
    }
}

/// A dashboard that parses at once. Like the real one, it acks every ACK_EVERY_BYTES and at the
/// end of a catch-up. The service holds its lock while it sends, so the ack waits in `pending`
/// until `deliver` sends it: the microtask of the Node test.
pub struct Watcher {
    inner: Arc<AckingClient>,
    pub client: ClientRef,
    terminal: String,
}

impl Watcher {
    pub fn attach(h: &Harness, terminal: &str, visible: bool) -> (Watcher, Detach) {
        let scheduler = Arc::clone(&h.scheduler);
        let inner = Arc::new(AckingClient {
            frames: Mutex::default(),
            arrivals: Mutex::default(),
            state: Mutex::default(),
            now: Box::new(move || scheduler.now()),
        });
        let client: ClientRef = inner.clone();
        let detach = h.attach_to(
            TASK,
            &client,
            SIZE,
            terminal,
            AttachOptions {
                visible: Some(visible),
                ..AttachOptions::default()
            },
        );
        let watcher = Watcher {
            inner,
            client,
            terminal: terminal.into(),
        };
        watcher.deliver(h);
        (watcher, detach)
    }

    pub fn frames(&self) -> Vec<Frame> {
        self.inner.frames.lock().unwrap().clone()
    }

    pub fn output(&self) -> String {
        String::from_utf8_lossy(&bytes_of(&self.frames())).into_owned()
    }

    /// The live output and when it came: (time, bytes).
    pub fn arrivals(&self) -> Vec<(u64, u64)> {
        self.inner.arrivals.lock().unwrap().clone()
    }

    pub fn deliver(&self, h: &Harness) {
        loop {
            let pending = std::mem::take(&mut self.inner.state.lock().unwrap().pending);
            if pending.is_empty() {
                return;
            }
            for at in pending {
                h.service.ack(TASK, &self.terminal, &self.client, at);
            }
        }
    }
}

/// `vi.advanceTimersByTimeAsync`: the clock moves on by the millisecond, and the acks of the
/// watchers go out between steps.
pub fn adv(h: &Harness, watchers: &[&Watcher], ms: u64) {
    let deliver = || watchers.iter().for_each(|watcher| watcher.deliver(h));
    h.pump();
    deliver();
    for _ in 0..ms {
        h.scheduler.advance(1);
        h.pump();
        deliver();
    }
    h.scheduler.advance(0);
    h.pump();
    deliver();
}

/// Output of `bytes` every 10 ms, for `ms`, as a program that keeps printing.
pub fn flood(h: &Harness, watchers: &[&Watcher], pty: &FakePty, ms: u64, bytes: usize) {
    let mut spent = 0;
    while spent < ms {
        pty.emit(format!("{}\r\n", "f".repeat(bytes - 2)));
        adv(h, watchers, 10);
        spent += 10;
    }
}

pub fn size_of(cols: u16, rows: u16) -> Size {
    Size { cols, rows }
}
