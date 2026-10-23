//! The state of a shell and of each client that shows it (`terminal-types.ts`).

use std::collections::VecDeque;
use std::fmt;
use std::sync::{Arc, Mutex, Weak};

use natamux_proto::terminals::messages::ServerMessage;
use natamux_pty::Pty;
use natamux_pty::input_gap::InputKind;
use natamux_screen::Screen;

use crate::shared::scheduler::{Scheduler, TimerId};

/// A binary frame (output bytes for xterm) or a control message (sent as JSON text).
#[derive(Clone, Debug, PartialEq)]
pub enum Frame {
    Bytes(Arc<[u8]>),
    Message(ServerMessage),
}

/// One socket that shows a shell. `send` never blocks.
pub trait TerminalClient: Send + Sync {
    fn send(&self, frame: Frame);
}

pub type ClientRef = Arc<dyn TerminalClient>;

pub fn same_client(a: &ClientRef, b: &ClientRef) -> bool {
    std::ptr::eq(Arc::as_ptr(a).cast::<()>(), Arc::as_ptr(b).cast::<()>())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Size {
    pub cols: u16,
    pub rows: u16,
}

/// The size of a shell started before any dashboard looks at it. The first resize fixes it.
pub const DEFAULT_SIZE: Size = Size {
    cols: 120,
    rows: 32,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TerminalError {
    /// The task has no workspace (or no task): `WorkspaceConflictError`, a 409.
    NoWorkspace(String),
    NoTerminal(String),
    ShellGone {
        task_id: String,
        terminal: String,
    },
    Limit(usize),
    /// The shell could not start.
    Spawn(String),
}

impl fmt::Display for TerminalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TerminalError::NoWorkspace(task_id) => write!(f, "{task_id} has no workspace"),
            TerminalError::NoTerminal(task_id) => {
                write!(f, "{task_id} has no open terminal. Start the agent first")
            }
            TerminalError::ShellGone { task_id, terminal } => {
                write!(f, "Terminal {terminal} of {task_id} has ended")
            }
            TerminalError::Limit(limit) => {
                write!(f, "At most {limit} terminals can be open. Close one first.")
            }
            TerminalError::Spawn(message) => write!(f, "{message}"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// Until its `replayed` is sent: its live output is dropped, the snapshot has it.
    Snapshot,
    Live,
    /// Its live output is dropped until it is caught up.
    Behind,
}

#[derive(Clone, Debug)]
pub struct ClientState {
    /// Unique per attach, so the detach of an old attach never removes a newer one.
    pub attach: u64,
    pub size: Size,
    /// Set once it claimed the size. Only such clients give the shell their size back.
    pub claimed: bool,
    pub visible: bool,
    pub phase: Phase,
    /// Offsets: the end of the output sent to it, and of the output it acked. `acked_at` is when
    /// its acks last moved on (or it was caught up).
    pub sent: u64,
    pub acked: u64,
    pub acked_at: u64,
    /// The first byte it missed, and when.
    pub behind_from: Option<u64>,
    pub behind_since: u64,
    /// When it last asked for a snapshot.
    pub snapshot_at: u64,
    /// The window of the hidden budget it is in, and the live output it got in it.
    pub budget_from: u64,
    pub budget_bytes: u64,
}

/// A snapshot that waits for a program to finish a synchronized frame.
pub struct SnapshotJob {
    pub clients: Vec<ClientRef>,
    pub timer: Option<TimerId>,
}

/// Input that waits for its turn, see `session_input::write`.
pub struct Input {
    pub data: String,
    pub kind: InputKind,
}

pub type SessionRef = Arc<Mutex<Session>>;

pub struct Session {
    pub task_id: String,
    pub terminal: String,
    /// Random, set when the shell starts, so a client can tell a new shell from the one it had.
    pub shell_id: String,
    pub pty: Box<dyn Pty>,
    pub screen: Screen,
    /// The client used last is the last one.
    pub clients: Vec<(ClientRef, ClientState)>,
    pub size: Size,
    /// Bytes of all output frames sent so far.
    pub offset: u64,
    /// Output not sent yet, see `queue_output`.
    pub batch: Vec<u8>,
    pub last_flush_at: Option<u64>,
    pub flush_timer: Option<TimerId>,
    /// The last frames sent, at most OUTPUT_RING_BYTES (or one frame), from offset `ring_start`.
    pub ring: VecDeque<Arc<[u8]>>,
    pub ring_bytes: usize,
    pub ring_start: u64,
    pub snapshot: Option<SnapshotJob>,
    pub paused: bool,
    /// Output of the last RESYNC_QUIET_WINDOW_MS, to tell when the shell is quiet.
    pub recent: VecDeque<(u64, u64)>,
    pub recent_bytes: u64,
    pub resync_timer: Option<TimerId>,
    pub kill_timer: Option<TimerId>,
    /// Set once the shell is being closed and the SIGKILL went out or is not needed.
    pub kill_done: bool,
    pub exited: bool,
    pub idle_timer: Option<TimerId>,
    pub pending: VecDeque<Input>,
    pub last_write: Option<(u64, InputKind)>,
    pub write_timer: Option<TimerId>,
    /// The first bytes of a character that the last read cut.
    pub partial: Vec<u8>,
    /// The program in the foreground when the output last went quiet.
    pub program: Option<String>,
    pub scheduler: Arc<Scheduler>,
    /// This session, for its timers.
    pub this: Weak<Mutex<Session>>,
}

impl Session {
    pub fn now(&self) -> u64 {
        self.scheduler.now()
    }

    pub fn client_state(&mut self, client: &ClientRef) -> Option<&mut ClientState> {
        self.clients
            .iter_mut()
            .find(|(known, _)| same_client(known, client))
            .map(|(_, state)| state)
    }

    /// Runs `job` on this session after `delay`, if it still exists then.
    pub fn later(&self, delay: u64, job: impl FnOnce(&mut Session) + Send + 'static) -> TimerId {
        let this = self.this.clone();
        self.scheduler.after(delay, move || {
            if let Some(session) = this.upgrade()
                && let Ok(mut session) = session.lock()
            {
                job(&mut session);
            }
        })
    }
}
