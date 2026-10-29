//! `terminal-bench.test.ts` for the Rust daemon: the same cases and field names, printed as
//! `TERMINAL_BENCH_JSON` for `scripts/compare-terminal-bench.py`. It runs in the daemon-bench CI
//! job only (`--ignored`, release build), on the real clock.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use natamux_daemon::shared::scheduler::Scheduler;
use natamux_daemon::terminals::service::{AttachOptions, TerminalOptions, TerminalService};
use natamux_daemon::terminals::types::{ClientRef, Frame, Size, TerminalClient};
use natamux_proto::terminals::history::scrollback_lines;
use natamux_proto::terminals::messages::{ACK_EVERY_BYTES, ServerMessage};
use natamux_pty::{Pty, PtyEvents, Signal, SpawnOptions};
use serde_json::{Value, json};

const HISTORY_LINES: usize = 10_000;
const SCREEN_ROWS: u16 = 40;
const PIECE_CHARS: usize = 64 * 1024;
const PARSE_BYTES: usize = 16 * 1024 * 1024;
const FLOOD_PIECE_CHARS: usize = 4096;
const FLOOD_LINES: usize = 300_000;
const TUI_REDRAWS: usize = 300;
const FLOOD_PIECES_PER_TURN: usize = 8;
const TASK: &str = "T-0001";

/// A pty whose output the bench gives.
type Sink = Arc<Mutex<Option<Box<dyn FnMut(Vec<u8>) + Send>>>>;

struct BenchPty(Sink);

impl Pty for BenchPty {
    fn start(&mut self, events: PtyEvents) {
        *self.0.lock().unwrap() = Some(events.on_data);
    }
    fn write(&mut self, _data: &[u8]) {}
    fn resize(&mut self, _cols: u16, _rows: u16) {}
    fn pause(&mut self) {}
    fn resume(&mut self) {}
    fn kill(&mut self, _signal: Signal) {}
}

fn setup() -> (TerminalService, Sink) {
    let sink: Sink = Arc::default();
    let shared = Arc::clone(&sink);
    let spawn = move |_: SpawnOptions| -> std::io::Result<Box<dyn Pty>> {
        Ok(Box::new(BenchPty(Arc::clone(&shared))))
    };
    let service = TerminalService::new(TerminalOptions {
        spawn: Arc::new(spawn),
        workspace_of: Arc::new(|task| Ok(format!("/work/{task}").into())),
        env: [("PATH".to_string(), "/bin".to_string())]
            .into_iter()
            .collect(),
        scheduler: Scheduler::real(),
        on_output: None,
        is_ended: None,
        bus: None,
        on_output_idle: None,
        on_exit: None,
        trace_input: false,
    });
    (service, sink)
}

fn emit(sink: &Sink, text: &str) {
    if let Some(on_data) = sink.lock().unwrap().as_mut() {
        on_data(text.as_bytes().to_vec());
    }
}

#[derive(Default)]
struct Stats {
    frames: u64,
    raw: u64,
    wire: u64,
    first: Option<Instant>,
    last: Option<Instant>,
    tail: String,
}

/// Records the binary frames. Acks as the dashboard does, from another thread, so flow control
/// never puts it behind.
#[derive(Default)]
struct Timed {
    stats: Mutex<Stats>,
    offset: AtomicU64,
    to_ack: AtomicU64,
    live: AtomicBool,
}

impl TerminalClient for Timed {
    fn send(&self, frame: Frame) {
        let bytes = match frame {
            Frame::Message(ServerMessage::Replayed { offset }) => {
                self.offset.store(offset, Ordering::SeqCst);
                self.to_ack.store(offset, Ordering::SeqCst);
                self.live.store(true, Ordering::SeqCst);
                return;
            }
            Frame::Message(_) => return,
            Frame::Bytes(bytes) => bytes,
        };
        if self.live.load(Ordering::SeqCst) {
            let offset =
                self.offset.fetch_add(bytes.len() as u64, Ordering::SeqCst) + bytes.len() as u64;
            if offset - self.to_ack.load(Ordering::SeqCst) >= ACK_EVERY_BYTES {
                self.to_ack.store(offset, Ordering::SeqCst);
            }
        }
        let now = Instant::now();
        let mut stats = self.stats.lock().unwrap();
        stats.first.get_or_insert(now);
        stats.last = Some(now);
        stats.frames += 1;
        stats.raw += bytes.len() as u64;
        let header = match bytes.len() {
            0..=125 => 2,
            126..=65_535 => 4,
            _ => 10,
        };
        stats.wire += bytes.len() as u64 + header;
        let text = format!("{}{}", stats.tail, String::from_utf8_lossy(&bytes));
        stats.tail = text[text.len().saturating_sub(1024)..].to_string();
    }
}

fn plain_line(index: usize, cols: usize) -> String {
    let head = format!("{index:06} ");
    format!(
        "{head}{}\r\n",
        "x".repeat((cols - 1).saturating_sub(head.len()))
    )
}

fn sgr_line(index: usize, cols: usize) -> String {
    let (mut line, mut visible, mut cell) = (String::new(), 0, index);
    while visible < cols - 1 {
        let count = 8.min(cols - 1 - visible);
        line += &format!(
            "\x1b[38;5;{}m\x1b[48;5;{}m{}",
            cell % 256,
            (cell * 7) % 256,
            "y".repeat(count)
        );
        visible += count;
        cell += 1;
    }
    format!("{line}\x1b[0m\r\n")
}

fn line(style: &str, index: usize, cols: usize) -> String {
    if style == "plain" {
        plain_line(index, cols)
    } else {
        sgr_line(index, cols)
    }
}

fn history(cols: usize, style: &str) -> String {
    let total = HISTORY_LINES + usize::from(SCREEN_ROWS);
    let mut text: String = (1..total).map(|index| line(style, index, cols)).collect();
    text += &format!("END-{cols}-{style}\r\n");
    text
}

fn pieces(text: &str, size: usize) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    while start < text.len() {
        let mut end = (start + size).min(text.len());
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        out.push(&text[start..end]);
        start = end;
    }
    out
}

fn ms(duration: Duration) -> f64 {
    (duration.as_secs_f64() * 10_000.0).round() / 10.0
}

fn attach(
    service: &TerminalService,
    client: &Arc<Timed>,
    cols: u16,
) -> natamux_daemon::terminals::shells::Detach {
    let client: ClientRef = client.clone();
    let size = Size {
        cols,
        rows: SCREEN_ROWS,
    };
    service
        .attach(TASK, client, size, "1", AttachOptions::default())
        .unwrap()
}

fn snapshot_case(cols: u16, style: &str) -> Value {
    let (service, sink) = setup();
    attach(&service, &Arc::default(), cols).detach();
    for part in pieces(&history(usize::from(cols), style), PIECE_CHARS) {
        emit(&sink, part);
    }
    service.settled(TASK, "1");
    let client: Arc<Timed> = Arc::default();
    let t0 = Instant::now();
    let _detach = attach(&service, &client, cols);
    let held = t0.elapsed();
    let stats = client.stats.lock().unwrap();
    assert!(stats.tail.contains(&format!("END-{cols}-{style}")));
    let at = |instant: Option<Instant>| ms(instant.unwrap().duration_since(t0));
    service.close_all();
    json!({
        "measure": "snapshot", "cols": cols, "style": style,
        "historyRows": scrollback_lines(cols), "snapshotBytes": stats.raw, "wireBytes": stats.wire,
        "frames": stats.frames, "serializeMs": at(stats.first), "attachMs": at(stats.last),
        // The time the shell was held, as no event loop is shared in Rust.
        "maxLoopDelayMs": ms(held),
    })
}

fn parse_case(style: &str) -> Value {
    let (service, sink) = setup();
    attach(&service, &Arc::default(), 120).detach();
    let block: String = (0..200).map(|index| line(style, index, 120)).collect();
    let parts = pieces(&block, PIECE_CHARS);
    let mut bytes = 0;
    let t0 = Instant::now();
    while bytes < PARSE_BYTES {
        for part in &parts {
            emit(&sink, part);
            bytes += part.len();
        }
    }
    service.settled(TASK, "1");
    let seconds = t0.elapsed().as_secs_f64();
    service.close_all();
    json!({
        "measure": "parse", "style": style, "megabytes": (bytes as f64 / 1e5).round() / 10.0,
        "seconds": (seconds * 100.0).round() / 100.0,
        "mbPerSecond": (bytes as f64 / 1e6 / seconds * 10.0).round() / 10.0,
    })
}

fn live_case(measure: &str, chunks: &[String]) -> Value {
    let (service, sink) = setup();
    let client: Arc<Timed> = Arc::default();
    let _detach = attach(&service, &client, 120);
    let acker = Arc::clone(&client);
    let acking = service.clone();
    let done = Arc::new(AtomicBool::new(false));
    let stop = Arc::clone(&done);
    let client_ref: ClientRef = client.clone();
    let thread = std::thread::spawn(move || {
        while !stop.load(Ordering::SeqCst) {
            acking.ack(TASK, "1", &client_ref, acker.to_ack.load(Ordering::SeqCst));
            std::thread::sleep(Duration::from_micros(200));
        }
    });
    let before = client.stats.lock().unwrap().frames;
    let raw_before = client.stats.lock().unwrap().raw;
    let t0 = Instant::now();
    // As the Node bench, the pty gives FLOOD_PIECES_PER_TURN pieces, then the acks get a turn.
    for (index, chunk) in chunks.iter().enumerate() {
        emit(&sink, chunk);
        if (index + 1) % FLOOD_PIECES_PER_TURN == 0 {
            std::thread::sleep(Duration::from_micros(500));
        }
    }
    std::thread::sleep(Duration::from_millis(50));
    service.settled(TASK, "1");
    done.store(true, Ordering::SeqCst);
    thread.join().unwrap();
    let stats = client.stats.lock().unwrap();
    let frames = stats.frames - before;
    let raw = stats.raw - raw_before;
    let seconds = stats
        .last
        .map_or(1e-6, |last| last.duration_since(t0).as_secs_f64().max(1e-6));
    service.close_all();
    json!({
        "measure": measure, "frames": frames,
        "bytesPerFrame": raw.checked_div(frames).unwrap_or(0),
        "wireOverRaw": ((stats.wire as f64 / stats.raw.max(1) as f64) * 1000.0).round() / 1000.0,
        "framesPerSecond": (frames as f64 / seconds * 10.0).round() / 10.0,
    })
}

#[test]
#[ignore = "a benchmark: the daemon-bench CI job runs it with --release"]
fn terminal_benchmark() {
    let mut results = Vec::new();
    for cols in [80, 120, 200] {
        for style in ["plain", "sgr"] {
            results.push(snapshot_case(cols, style));
        }
    }
    for style in ["plain", "sgr"] {
        results.push(parse_case(style));
    }
    let flood: String = (1..=FLOOD_LINES)
        .map(|index| format!("{index}\r\n"))
        .collect();
    let chunks: Vec<String> = pieces(&flood, FLOOD_PIECE_CHARS)
        .into_iter()
        .map(String::from)
        .collect();
    results.push(live_case("flood", &chunks));
    let redraws: Vec<String> = (0..TUI_REDRAWS)
        .map(|step| {
            let rows: Vec<String> = (0..usize::from(SCREEN_ROWS))
                .map(|row| {
                    format!(
                        "\x1b[38;5;{}m{}\x1b[0m",
                        (step + row) % 256,
                        "z".repeat(119)
                    )
                })
                .collect();
            format!("\x1b[?2026h\x1b[H{}\x1b[?2026l", rows.join("\r\n"))
        })
        .collect();
    results.push(live_case("tui-redraw", &redraws));
    println!("TERMINAL_BENCH_JSON {}", Value::Array(results));
}
