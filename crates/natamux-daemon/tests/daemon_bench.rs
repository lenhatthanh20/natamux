//! The daemon numbers of T-0178, run only with `NATAMUX_BENCH=1` (the daemon-bench CI job, release
//! build): the memory of 30 idle shells, the echo delay of a quiet shell while 5 others flood, the
//! snapshot time of a full history, and the time and allocations of `serialize`. It writes a
//! table to the job summary before it checks the targets, so a number over its target still
//! shows.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use natamux_daemon::shared::scheduler::Scheduler;
use natamux_daemon::terminals::limits::MAX_SESSIONS;
use natamux_daemon::terminals::service::{AttachOptions, TerminalOptions, TerminalService};
use natamux_daemon::terminals::types::{ClientRef, Frame, Size, TerminalClient};
use natamux_proto::terminals::messages::ServerMessage;
use natamux_pty::{Pty, PtyEvents, Signal, SpawnOptions};
use natamux_screen::Screen;

const TASK: &str = "T-0001";
const COLS: u16 = 120;
const ROWS: u16 = 40;
const IDLE_SHELLS: usize = 30;
const FLOODING: usize = 5;
const ECHOES: usize = 200;
/// Spec 7.3: a quiet shell echoes within this while others flood.
const ECHO_P95_MS: f64 = 10.0;
const PROMPT: &str = "\x1b[1mbench\x1b[0m ~/work % ";

/// Counts the allocations of the whole test binary, for the `serialize` case.
struct Counting;

static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
static ALLOCATED: AtomicU64 = AtomicU64::new(0);

// SAFETY: every call goes to the system allocator with the same arguments.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        ALLOCATED.fetch_add(layout.size() as u64, Ordering::Relaxed);
        // SAFETY: the caller keeps the contract of `GlobalAlloc::alloc`.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: `ptr` came from this allocator, which is the system one.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        ALLOCATED.fetch_add(new_size as u64, Ordering::Relaxed);
        // SAFETY: as for `dealloc`, and the caller keeps the contract of `realloc`.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// The output side of each fake pty, in the order the shells opened.
type Sinks = Arc<Mutex<Vec<Sink>>>;
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

fn setup() -> (TerminalService, Sinks) {
    let sinks: Sinks = Arc::default();
    let shared = Arc::clone(&sinks);
    let spawn = move |_: SpawnOptions| -> std::io::Result<Box<dyn Pty>> {
        let sink: Sink = Arc::default();
        shared.lock().unwrap().push(Arc::clone(&sink));
        Ok(Box::new(BenchPty(sink)))
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
        trace_input: false,
    });
    (service, sinks)
}

fn emit(sink: &Sink, bytes: &[u8]) {
    if let Some(on_data) = sink.lock().unwrap().as_mut() {
        on_data(bytes.to_vec());
    }
}

/// A client that drops what it gets: the shell stays open after it leaves.
struct Nobody;

impl TerminalClient for Nobody {
    fn send(&self, _frame: Frame) {}
}

/// Tells when output comes once the catch-up ended.
struct Echoes {
    live: AtomicBool,
    heard: Mutex<Sender<Instant>>,
}

impl TerminalClient for Echoes {
    fn send(&self, frame: Frame) {
        match frame {
            Frame::Message(ServerMessage::Replayed { .. }) => {
                self.live.store(true, Ordering::SeqCst);
            }
            Frame::Bytes(_) if self.live.load(Ordering::SeqCst) => {
                self.heard.lock().unwrap().send(Instant::now()).ok();
            }
            _ => {}
        }
    }
}

fn size() -> Size {
    Size {
        cols: COLS,
        rows: ROWS,
    }
}

/// Opens shell `terminal` with a client that leaves at once.
fn open(service: &TerminalService, terminal: &str) {
    let client: ClientRef = Arc::new(Nobody);
    let options = AttachOptions::default();
    service
        .attach(TASK, client, size(), terminal, options)
        .unwrap()
        .detach();
}

fn rss_kib() -> Option<u64> {
    let pid = std::process::id().to_string();
    let out = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &pid])
        .output()
        .ok()?;
    String::from_utf8_lossy(&out.stdout).trim().parse().ok()
}

fn ms(duration: Duration) -> f64 {
    (duration.as_secs_f64() * 10_000.0).round() / 10.0
}

fn history() -> Vec<u8> {
    let lines = natamux_proto::terminals::history::scrollback_lines(COLS) + usize::from(ROWS);
    let mut text = String::new();
    for index in 0..lines {
        let head = format!("\x1b[3{}m{index:06}\x1b[0m ", index % 8);
        text += &head;
        text += &"x".repeat(usize::from(COLS) - 12);
        text += "\r\n";
    }
    text.into_bytes()
}

/// RSS of the daemon with 30 idle shells, against none.
fn idle_shells() -> (f64, f64) {
    let (service, sinks) = setup();
    let before = rss_kib().unwrap_or(0);
    let shells = IDLE_SHELLS.min(MAX_SESSIONS);
    for index in 1..=shells {
        open(&service, &index.to_string());
    }
    for sink in sinks.lock().unwrap().iter() {
        emit(sink, PROMPT.as_bytes());
    }
    std::thread::sleep(Duration::from_millis(200));
    let after = rss_kib().unwrap_or(0);
    service.close_all();
    let grown = after.saturating_sub(before) as f64;
    ((after as f64 / 1024.0 * 10.0).round() / 10.0, (grown / shells as f64).round())
}

/// The echo delay of a quiet shell while 5 others flood: p50 and p95 in ms.
fn echo_while_flooding() -> (f64, f64) {
    let (service, sinks) = setup();
    for index in 1..=FLOODING {
        open(&service, &format!("flood-{index}"));
    }
    let (heard, arrivals) = channel();
    let quiet = Arc::new(Echoes {
        live: AtomicBool::new(false),
        heard: Mutex::new(heard),
    });
    let client: ClientRef = quiet.clone();
    let _detach = service
        .attach(TASK, client, size(), "quiet", AttachOptions::default())
        .unwrap();
    let all: Vec<Sink> = sinks.lock().unwrap().clone();
    let stop = Arc::new(AtomicBool::new(false));
    let block: Vec<u8> = (0..600)
        .flat_map(|line| format!("{line:05} {}\r\n", "f".repeat(100)).into_bytes())
        .take(64 * 1024)
        .collect();
    let floods: Vec<_> = all[..FLOODING]
        .iter()
        .map(|sink| {
            let (sink, stop, block) = (Arc::clone(sink), Arc::clone(&stop), block.clone());
            std::thread::spawn(move || {
                while !stop.load(Ordering::SeqCst) {
                    emit(&sink, &block);
                }
            })
        })
        .collect();
    std::thread::sleep(Duration::from_millis(300));
    while arrivals.try_recv().is_ok() {}
    let mut delays = Vec::with_capacity(ECHOES);
    for _ in 0..ECHOES {
        let start = Instant::now();
        emit(&all[FLOODING], b"k");
        if let Ok(at) = arrivals.recv_timeout(Duration::from_secs(1)) {
            delays.push(ms(at.duration_since(start)));
        } else {
            delays.push(1000.0);
        }
        std::thread::sleep(Duration::from_millis(15));
        while arrivals.try_recv().is_ok() {}
    }
    stop.store(true, Ordering::SeqCst);
    for flood in floods {
        flood.join().unwrap();
    }
    service.close_all();
    delays.sort_by(f64::total_cmp);
    let at = |share: f64| delays[((delays.len() - 1) as f64 * share).round() as usize];
    (at(0.5), at(0.95))
}

/// The time a new client waits for the snapshot of a full history.
fn snapshot_of_full_history() -> f64 {
    let (service, sinks) = setup();
    open(&service, "1");
    let sink = Arc::clone(&sinks.lock().unwrap()[0]);
    for piece in history().chunks(64 * 1024) {
        emit(&sink, piece);
    }
    service.settled(TASK, "1");
    let client: ClientRef = Arc::new(Nobody);
    let start = Instant::now();
    let detach = service.attach(TASK, client, size(), "1", AttachOptions::default());
    let held = start.elapsed();
    drop(detach);
    service.close_all();
    ms(held)
}

/// `serialize` of a full history on its own: ms, allocations and MiB allocated.
fn serialize() -> (f64, u64, f64) {
    let mut screen = Screen::new(COLS, ROWS);
    screen.write(&history());
    let (count, bytes) = (
        ALLOCATIONS.load(Ordering::SeqCst),
        ALLOCATED.load(Ordering::SeqCst),
    );
    let start = Instant::now();
    let snapshot = screen.snapshot();
    let took = start.elapsed();
    let count = ALLOCATIONS.load(Ordering::SeqCst) - count;
    let bytes = ALLOCATED.load(Ordering::SeqCst) - bytes;
    assert!(!snapshot.is_empty());
    let mib = (bytes as f64 / 1024.0 / 1024.0 * 10.0).round() / 10.0;
    (ms(took), count, mib)
}

#[test]
fn daemon_benchmark() {
    if std::env::var("NATAMUX_BENCH").as_deref() != Ok("1") {
        eprintln!("daemon bench: skipped, set NATAMUX_BENCH=1 to run it");
        return;
    }
    // First, while no other thread allocates.
    let (serialize_ms, allocations, allocated) = serialize();
    let (rss_mib, per_shell_kib) = idle_shells();
    let (echo_p50, echo_p95) = echo_while_flooding();
    let snapshot_ms = snapshot_of_full_history();
    let table = format!(
        "| measure | value |\n|---|---|\n\
         | RSS with {IDLE_SHELLS} idle shells | {rss_mib} MiB ({per_shell_kib} KiB a shell) |\n\
         | echo of a quiet shell while {FLOODING} flood, p50 | {echo_p50} ms |\n\
         | echo of a quiet shell while {FLOODING} flood, p95 (target {ECHO_P95_MS}) | {echo_p95} ms |\n\
         | snapshot of a full history at {COLS} cols | {snapshot_ms} ms |\n\
         | serialize of a full history | {serialize_ms} ms, {allocations} allocations, {allocated} MiB |\n"
    );
    println!("{table}");
    if let Ok(path) = std::env::var("GITHUB_STEP_SUMMARY") {
        let old = std::fs::read_to_string(&path).unwrap_or_default();
        std::fs::write(&path, format!("{old}### Daemon bench\n\n{table}\n")).ok();
    }
    assert!(
        echo_p95 <= ECHO_P95_MS,
        "echo p95 {echo_p95} ms is over {ECHO_P95_MS} ms"
    );
}
