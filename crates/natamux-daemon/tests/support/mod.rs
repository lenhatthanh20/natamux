//! Test doubles for the terminal service: a fake pseudo terminal, a client that records what it
//! gets, and a harness on a manual clock (`terminal-fake.fixture.ts` and the helpers of
//! `terminal-service.test.ts`).
//!
//! The service writes to a pty and kills it while it holds the session's lock, so the fake cannot
//! echo or end at once as the TypeScript fake does: it queues that, and `Harness::pump` (called by
//! every harness step) delivers it.

// Each test file uses a part of it.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use natamux_daemon::shared::event_bus::{DaemonMessage, EventBus};
use natamux_daemon::shared::scheduler::Scheduler;
use natamux_daemon::terminals::service::{
    AttachOptions, OutputListener, ShellListener, TaskCheck, TaskListener, TerminalOptions,
    TerminalService,
};
use natamux_daemon::terminals::session_lifecycle::SpawnPty;
use natamux_daemon::terminals::shells::Detach;
use natamux_daemon::terminals::types::{ClientRef, Frame, TerminalClient, TerminalError};
use natamux_proto::terminals::messages::ServerMessage;
use natamux_pty::{Env, Pty, PtyEvents, Signal, SpawnOptions};
use natamux_screen::Screen;

pub use natamux_daemon::terminals::types::Size;

pub const SIZE: Size = Size { cols: 80, rows: 24 };

static NEXT_PID: AtomicU32 = AtomicU32::new(1000);

/// What a fake pty was asked, and what the test makes it do.
pub struct FakeState {
    pub pid: u32,
    pub options: SpawnOptions,
    pub written: Vec<String>,
    pub sizes: Vec<(u16, u16)>,
    pub killed: bool,
    pub signals: Vec<Signal>,
    /// A process that ignores everything but SIGKILL, like a program that traps SIGHUP.
    pub stubborn: bool,
    /// A program under the shell that ignores SIGHUP: the shell ends, it lives on until SIGKILL.
    pub orphan: bool,
    /// The program in the foreground; tests set it to pretend an agent runs.
    pub program: Option<String>,
    pub command_lines: Option<Vec<String>>,
    /// pause() and resume() calls, in order. emit() still delivers output while paused.
    pub flow: Vec<&'static str>,
    pub paused: bool,
    /// Echo of what was written, and an exit, not delivered yet.
    queued: Vec<Queued>,
    events: Option<PtyEvents>,
}

enum Queued {
    Data(Vec<u8>),
    Exit(Option<i32>),
}

/// The test's handle on a fake pty.
#[derive(Clone)]
pub struct FakePty(Arc<Mutex<FakeState>>);

struct PtySide(Arc<Mutex<FakeState>>);

fn lock(state: &Mutex<FakeState>) -> MutexGuard<'_, FakeState> {
    state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl Pty for PtySide {
    fn start(&mut self, events: PtyEvents) {
        lock(&self.0).events = Some(events);
    }

    fn write(&mut self, data: &[u8]) {
        let mut state = lock(&self.0);
        state
            .written
            .push(String::from_utf8_lossy(data).into_owned());
        state.queued.push(Queued::Data(data.to_vec()));
    }

    fn resize(&mut self, cols: u16, rows: u16) {
        lock(&self.0).sizes.push((cols, rows));
    }

    fn pause(&mut self) {
        let mut state = lock(&self.0);
        state.paused = true;
        state.flow.push("pause");
    }

    fn resume(&mut self) {
        let mut state = lock(&self.0);
        state.paused = false;
        state.flow.push("resume");
    }

    fn kill(&mut self, signal: Signal) {
        let mut state = lock(&self.0);
        state.signals.push(signal);
        if signal == Signal::Kill {
            state.orphan = false;
        }
        // A real shell reports its end once.
        if state.killed || (state.stubborn && signal != Signal::Kill) {
            return;
        }
        state.killed = true;
        state.queued.push(Queued::Exit(None));
    }

    fn leftover(&self) -> bool {
        lock(&self.0).orphan
    }

    fn foreground(&self) -> Option<String> {
        lock(&self.0).program.clone()
    }

    fn commands(&self) -> Option<Vec<String>> {
        lock(&self.0).command_lines.clone()
    }
}

impl FakePty {
    pub fn state(&self) -> MutexGuard<'_, FakeState> {
        lock(&self.0)
    }

    /// Bytes, as the real pty gives them, delivered now.
    pub fn emit(&self, data: impl AsRef<[u8]>) {
        let events = lock(&self.0).events.take();
        if let Some(mut events) = events {
            (events.on_data)(data.as_ref().to_vec());
            let mut state = lock(&self.0);
            if state.events.is_none() {
                state.events = Some(events);
            }
        }
    }

    /// The shell ends with `code`, now.
    pub fn exit(&self, code: Option<i32>) {
        let events = lock(&self.0).events.take();
        if let Some(events) = events {
            (events.on_exit)(code);
        }
    }

    pub fn written(&self) -> Vec<String> {
        self.state().written.clone()
    }

    pub fn sizes(&self) -> Vec<(u16, u16)> {
        self.state().sizes.clone()
    }

    pub fn signals(&self) -> Vec<Signal> {
        self.state().signals.clone()
    }

    pub fn killed(&self) -> bool {
        self.state().killed
    }

    pub fn env(&self, name: &str) -> Option<String> {
        self.state().options.env.get(name).cloned()
    }

    pub fn set_program(&self, program: Option<&str>) {
        self.state().program = program.map(String::from);
    }

    /// Delivers what was queued. True when something was.
    pub fn deliver(&self) -> bool {
        let queued = std::mem::take(&mut lock(&self.0).queued);
        let any = !queued.is_empty();
        for item in queued {
            match item {
                Queued::Data(data) => self.emit(data),
                Queued::Exit(code) => self.exit(code),
            }
        }
        any
    }
}

/// A client that records every frame.
#[derive(Default)]
pub struct Recorder {
    frames: Mutex<Vec<Frame>>,
}

impl TerminalClient for Recorder {
    fn send(&self, frame: Frame) {
        self.frames.lock().unwrap().push(frame);
    }
}

impl Recorder {
    pub fn frames(&self) -> Vec<Frame> {
        self.frames.lock().unwrap().clone()
    }

    pub fn clear(&self) {
        self.frames.lock().unwrap().clear();
    }

    /// All the output bytes, in order.
    pub fn bytes(&self) -> Vec<u8> {
        bytes_of(&self.frames())
    }

    /// All the output, as one text.
    pub fn output(&self) -> String {
        String::from_utf8_lossy(&self.bytes()).into_owned()
    }

    /// The control messages, in order.
    pub fn controls(&self) -> Vec<ServerMessage> {
        controls(&self.frames())
    }

    /// The screen a dashboard of `size` shows after this output: every row, history first,
    /// without the empty rows at the end.
    pub fn screen(&self, size: Size) -> Vec<String> {
        screen_of(&self.frames(), size)
    }
}

pub fn bytes_of(frames: &[Frame]) -> Vec<u8> {
    frames
        .iter()
        .filter_map(|frame| match frame {
            Frame::Bytes(bytes) => Some(bytes.to_vec()),
            Frame::Message(_) => None,
        })
        .flatten()
        .collect()
}

pub fn controls(frames: &[Frame]) -> Vec<ServerMessage> {
    frames
        .iter()
        .filter_map(|frame| match frame {
            Frame::Message(message) => Some(message.clone()),
            Frame::Bytes(_) => None,
        })
        .collect()
}

/// Like the dashboard, it draws a snapshot at the size the snapshot names, then fits back to its
/// own size.
pub fn screen_of(frames: &[Frame], size: Size) -> Vec<String> {
    let snapshot = controls(frames)
        .into_iter()
        .rev()
        .find_map(|message| match message {
            ServerMessage::Snapshot { cols, rows } => Some(Size { cols, rows }),
            _ => None,
        });
    let first = snapshot.unwrap_or(size);
    let mut screen = Screen::new(first.cols, first.rows);
    screen.write(&bytes_of(frames));
    screen.resize(size.cols, size.rows);
    let text = screen.last_lines(usize::MAX);
    if text.is_empty() {
        return Vec::new();
    }
    text.split('\n').map(String::from).collect()
}

/// The shell id of a hello, for comparing hellos.
pub fn hello_id(message: &ServerMessage) -> Option<String> {
    match message {
        ServerMessage::Hello { shell_id } => Some(shell_id.clone()),
        _ => None,
    }
}

pub fn is_hello(message: &ServerMessage) -> bool {
    hello_id(message).is_some()
}

/// The lines "line 1" to "line <count>", as a shell prints them.
pub fn numbered(count: usize) -> String {
    (1..=count)
        .map(|index| format!("line {index}\r\n"))
        .collect()
}

/// The same lines as a screen shows them.
pub fn numbered_lines(count: usize) -> Vec<String> {
    (1..=count).map(|index| format!("line {index}")).collect()
}

#[derive(Default)]
pub struct SetupOptions {
    pub is_ended: Option<Arc<TaskCheck>>,
    pub on_output: Option<Arc<OutputListener>>,
    pub on_output_idle: Option<Arc<TaskListener>>,
    pub on_exit: Option<Arc<ShellListener>>,
    pub env: Option<Env>,
}

pub struct Harness {
    pub service: TerminalService,
    pub scheduler: Arc<Scheduler>,
    pub bus: Arc<EventBus>,
    /// Every message on the bus, in order.
    pub messages: Arc<Mutex<Vec<DaemonMessage>>>,
    ptys: Arc<Mutex<Vec<FakePty>>>,
}

pub fn setup() -> Harness {
    setup_with(SetupOptions::default())
}

/// The service of `terminal-service.test.ts`: T-0009 has no workspace, every other task's is
/// `/work/<id>`.
pub fn setup_with(options: SetupOptions) -> Harness {
    let scheduler = Scheduler::manual();
    let bus = EventBus::new();
    let messages = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&messages);
    bus.subscribe(move |message| sink.lock().unwrap().push(message.clone()));
    let (spawn, ptys) = fake_spawner();
    let env = options.env.unwrap_or_else(|| {
        [("PATH", "/bin"), ("NATAMUX_TOKEN", "secret")]
            .iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect()
    });
    let service = TerminalService::new(TerminalOptions {
        spawn,
        workspace_of: Arc::new(|task_id: &str| {
            if task_id == "T-0009" {
                Err(TerminalError::NoWorkspace(task_id.into()))
            } else {
                Ok(format!("/work/{task_id}").into())
            }
        }),
        env,
        scheduler: Arc::clone(&scheduler),
        on_output: options.on_output,
        is_ended: options.is_ended,
        bus: Some(Arc::clone(&bus)),
        on_output_idle: options.on_output_idle,
        on_exit: options.on_exit,
        trace_input: false,
    });
    Harness {
        service,
        scheduler,
        bus,
        messages,
        ptys,
    }
}

/// Makes a spawn fail with "No shell" when it answers true for (task id, terminal name).
pub type SpawnGate = Arc<Mutex<Option<Arc<dyn Fn(&str, &str) -> bool + Send + Sync>>>>;

/// A spawner of fake ptys, and the list of those it made, in order.
pub fn fake_spawner() -> (Arc<SpawnPty>, Arc<Mutex<Vec<FakePty>>>) {
    fake_spawner_gated(SpawnGate::default())
}

pub fn fake_spawner_gated(gate: SpawnGate) -> (Arc<SpawnPty>, Arc<Mutex<Vec<FakePty>>>) {
    let ptys: Arc<Mutex<Vec<FakePty>>> = Arc::new(Mutex::new(Vec::new()));
    let made = Arc::clone(&ptys);
    let spawn = move |options: SpawnOptions| -> std::io::Result<Box<dyn Pty>> {
        let refuse = gate.lock().unwrap().clone();
        if let Some(refuse) = refuse {
            let named = |name: &str| options.env.get(name).cloned().unwrap_or_default();
            if refuse(&named("NATAMUX_TASK_ID"), &named("NATAMUX_TERMINAL")) {
                return Err(std::io::Error::other("No shell"));
            }
        }
        let state = Arc::new(Mutex::new(FakeState {
            pid: NEXT_PID.fetch_add(1, Ordering::SeqCst),
            options,
            written: Vec::new(),
            sizes: Vec::new(),
            killed: false,
            signals: Vec::new(),
            stubborn: false,
            orphan: false,
            program: Some("bash".into()),
            command_lines: None,
            flow: Vec::new(),
            paused: false,
            queued: Vec::new(),
            events: None,
        }));
        made.lock().unwrap().push(FakePty(Arc::clone(&state)));
        Ok(Box::new(PtySide(state)))
    };
    (Arc::new(spawn), ptys)
}

pub fn client() -> (Arc<Recorder>, ClientRef) {
    let recorder = Arc::new(Recorder::default());
    let client: ClientRef = recorder.clone();
    (recorder, client)
}

impl Harness {
    pub fn ptys(&self) -> Vec<FakePty> {
        self.ptys.lock().unwrap().clone()
    }

    pub fn pty(&self, index: usize) -> FakePty {
        self.ptys()[index].clone()
    }

    /// Delivers the echo and the exits the fakes queued, until none is left.
    pub fn pump(&self) {
        loop {
            let mut any = false;
            for pty in self.ptys() {
                any |= pty.deliver();
            }
            if !any {
                return;
            }
        }
    }

    /// Moves the clock on, running the timers that fall due, with echoes delivered on the way.
    pub fn advance(&self, ms: u64) {
        self.pump();
        for _ in 0..ms {
            self.scheduler.advance(1);
            self.pump();
        }
        self.scheduler.advance(0);
        self.pump();
    }

    /// Runs what is due now, as a turn of the Node event loop.
    pub fn tick(&self) {
        self.advance(0);
    }

    /// The output so far is sent.
    pub fn settled(&self, task_id: &str, terminal: &str) {
        self.pump();
        self.tick();
        self.service.settled(task_id, terminal);
    }

    /// Attaches a visible client of SIZE to terminal "1" of T-0001.
    pub fn attach(&self, client: &ClientRef) -> Detach {
        self.attach_to("T-0001", client, SIZE, "1", AttachOptions::default())
    }

    pub fn attach_to(
        &self,
        task_id: &str,
        client: &ClientRef,
        size: Size,
        terminal: &str,
        options: AttachOptions,
    ) -> Detach {
        let detach = self
            .service
            .attach(task_id, Arc::clone(client), size, terminal, options)
            .unwrap_or_else(|error| panic!("attach failed: {error}"));
        self.pump();
        detach
    }

    pub fn messages(&self) -> Vec<DaemonMessage> {
        self.messages.lock().unwrap().clone()
    }
}

pub fn shells_changed(task_id: &str) -> DaemonMessage {
    DaemonMessage::ShellsChanged {
        task_id: task_id.into(),
    }
}

pub fn programs_changed(task_id: &str, terminals: &[&str]) -> DaemonMessage {
    DaemonMessage::ProgramsChanged {
        task_id: task_id.into(),
        terminals: terminals.iter().map(|name| name.to_string()).collect(),
    }
}
