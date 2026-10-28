//! The daemon numbers of T-0178, run only with `NATAMUX_BENCH=1` (the daemon-bench CI job, release
//! build): the memory of 30 idle shells, the echo delay of a quiet shell while 5 others flood or
//! 29 agents print (T-0210), the snapshot time of a full history, and the time and allocations of
//! `serialize`. It writes a table to the job summary before it checks the targets, so a number
//! over its target still shows.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use natamux_daemon::shared::scheduler::Scheduler;
use natamux_daemon::terminals::limits::MAX_SESSIONS;
use natamux_daemon::terminals::service::{AttachOptions, TerminalOptions, TerminalService};
use natamux_daemon::terminals::types::{ClientRef, Frame, Size, TerminalClient};
use natamux_proto::terminals::messages::{ACK_EVERY_BYTES, ServerMessage};
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
/// T-0210: 30 agents at work, one of them typed in.
const AGENTS: usize = 29;
const AGENT_ECHO_P95_MS: f64 = 5.0;
/// An agent's TUI redraws this often, and scrolls a long answer this often.
const AGENT_FRAME_MS: u64 = 100;
const AGENT_SCROLL_EVERY: u64 = 20;
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
    (
        (after as f64 / 1024.0 * 10.0).round() / 10.0,
        (grown / shells as f64).round(),
    )
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
    let measured = echoes(&all[FLOODING], &arrivals);
    stop.store(true, Ordering::SeqCst);
    for flood in floods {
        flood.join().unwrap();
    }
    service.close_all();
    measured
}

/// Types into the shell of `sink` while the others print: p50 and p95 of the echo in ms.
fn echoes(sink: &Sink, arrivals: &Receiver<Instant>) -> (f64, f64) {
    std::thread::sleep(Duration::from_millis(300));
    while arrivals.try_recv().is_ok() {}
    let mut delays = Vec::with_capacity(ECHOES);
    for _ in 0..ECHOES {
        let start = Instant::now();
        emit(sink, b"k");
        if let Ok(at) = arrivals.recv_timeout(Duration::from_secs(1)) {
            delays.push(ms(at.duration_since(start)));
        } else {
            delays.push(1000.0);
        }
        std::thread::sleep(Duration::from_millis(15));
        while arrivals.try_recv().is_ok() {}
    }
    delays.sort_by(f64::total_cmp);
    let at = |share: f64| delays[((delays.len() - 1) as f64 * share).round() as usize];
    (at(0.5), at(0.95))
}

/// What an acking dashboard owes the daemon.
#[derive(Default)]
struct AckState {
    live: bool,
    got: u64,
    acked: u64,
    due: Option<u64>,
}

/// A shown pane that parses at once. Like the app, it acks every ACK_EVERY_BYTES and at the end
/// of a catch-up. The service holds the session's lock while it sends, so the ack waits in `due`.
#[derive(Default)]
struct Acking(Mutex<AckState>);

impl TerminalClient for Acking {
    fn send(&self, frame: Frame) {
        let mut state = self.0.lock().unwrap();
        match frame {
            Frame::Message(ServerMessage::Replayed { offset }) => {
                state.live = true;
                state.got = offset;
                state.acked = offset;
                state.due = Some(offset);
            }
            Frame::Bytes(bytes) if state.live => {
                state.got += bytes.len() as u64;
                if state.got - state.acked >= ACK_EVERY_BYTES {
                    state.acked = state.got;
                    state.due = Some(state.got);
                }
            }
            _ => {}
        }
    }
}

/// One redraw of an agent's TUI: a synchronized frame of 20 coloured rows over the last ones.
fn agent_frame(tick: u64) -> Vec<u8> {
    let mut text = String::from("\x1b[?2026h\x1b[20A");
    for row in 0..20 {
        let colour = 31 + row % 6;
        text += &format!("\r\x1b[2K\x1b[{colour}m{tick:08} row {row:02} \x1b[0m");
        text += &"a".repeat(usize::from(COLS) - 30);
        text += "\r\n";
    }
    text += "\x1b[?2026l";
    text.into_bytes()
}

/// A long answer of an agent: 200 lines that scroll.
fn agent_scroll() -> Vec<u8> {
    (0..200)
        .flat_map(|line| format!("{line:05} {}\r\n", "s".repeat(100)).into_bytes())
        .collect()
}

/// The echo delay of a typed shell while 29 agents print, each shown in an acking pane: p50 and
/// p95 in ms.
fn echo_while_agents_print() -> (f64, f64) {
    let (service, sinks) = setup();
    let mut panes = Vec::new();
    for index in 1..=AGENTS {
        let acking = Arc::new(Acking::default());
        let client: ClientRef = acking.clone();
        let terminal = format!("agent-{index}");
        let options = AttachOptions::default();
        let detach = service
            .attach(TASK, Arc::clone(&client), size(), &terminal, options)
            .unwrap();
        panes.push((acking, client, terminal, detach));
    }
    let (heard, arrivals) = channel();
    let typed = Arc::new(Echoes {
        live: AtomicBool::new(false),
        heard: Mutex::new(heard),
    });
    let client: ClientRef = typed.clone();
    let _detach = service
        .attach(TASK, client, size(), "typed", AttachOptions::default())
        .unwrap();
    let all: Vec<Sink> = sinks.lock().unwrap().clone();
    let stop = Arc::new(AtomicBool::new(false));
    let scroll = agent_scroll();
    let agents: Vec<_> = panes
        .iter()
        .enumerate()
        .map(|(index, (acking, client, terminal, _))| {
            let (sink, stop) = (Arc::clone(&all[index]), Arc::clone(&stop));
            let (scroll, service) = (scroll.clone(), service.clone());
            let (acking, client) = (Arc::clone(acking), Arc::clone(client));
            let terminal = terminal.clone();
            // Agents do not redraw in step: each starts at its own point of the 100 ms.
            let offset = index as u64 * AGENT_FRAME_MS / AGENTS as u64;
            let start = Instant::now() + Duration::from_millis(offset);
            std::thread::spawn(move || {
                let mut tick = 0;
                while !stop.load(Ordering::SeqCst) {
                    let due = start + Duration::from_millis(tick * AGENT_FRAME_MS);
                    std::thread::sleep(due.saturating_duration_since(Instant::now()));
                    emit(&sink, &agent_frame(tick));
                    if tick % AGENT_SCROLL_EVERY == AGENT_SCROLL_EVERY - 1 {
                        emit(&sink, &scroll);
                    }
                    let owed = acking.0.lock().unwrap().due.take();
                    if let Some(offset) = owed {
                        service.ack(TASK, &terminal, &client, offset);
                    }
                    tick += 1;
                }
            })
        })
        .collect();
    let measured = echoes(&all[AGENTS], &arrivals);
    stop.store(true, Ordering::SeqCst);
    for agent in agents {
        agent.join().unwrap();
    }
    drop(panes);
    service.close_all();
    measured
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
    let (agent_p50, agent_p95) = echo_while_agents_print();
    let snapshot_ms = snapshot_of_full_history();
    let table = format!(
        "| measure | value |\n|---|---|\n\
         | RSS with {IDLE_SHELLS} idle shells | {rss_mib} MiB ({per_shell_kib} KiB a shell) |\n\
         | echo of a quiet shell while {FLOODING} flood, p50 | {echo_p50} ms |\n\
         | echo of a quiet shell while {FLOODING} flood, p95 (target {ECHO_P95_MS}) | {echo_p95} ms |\n\
         | echo of a typed shell while {AGENTS} agents print, p50 | {agent_p50} ms |\n\
         | echo of a typed shell while {AGENTS} agents print, p95 (target {AGENT_ECHO_P95_MS}) | {agent_p95} ms |\n\
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
    assert!(
        agent_p95 <= AGENT_ECHO_P95_MS,
        "echo p95 with {AGENTS} agents {agent_p95} ms is over {AGENT_ECHO_P95_MS} ms"
    );
}
