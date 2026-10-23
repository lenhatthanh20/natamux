//! The numbers of `terminal-limits.ts`, in milliseconds and bytes. `tests/limits.rs` checks them
//! against the TypeScript.

pub use natamux_proto::terminals::list::SHELL_PROGRAMS;
pub use natamux_proto::terminals::messages::MAIN_TERMINAL;
pub use natamux_pty::input_gap::{ENTER_AFTER_PASTE_MS, INPUT_GAP_MS};

/// A task of the default pipeline keeps three agent shells (1, 2 and 5), so ten tasks fit.
pub const MAX_SESSIONS: usize = 30;
pub const TICKET_TTL_MS: u64 = 30_000;
/// Output is sent in batches. A chunk that comes after a quiet moment and is small (typing echo)
/// goes out at once; anything else waits this long for more.
pub const OUTPUT_COALESCE_MS: u64 = 4;
pub const OUTPUT_IDLE_BYTES: usize = 1024;
/// No frame is larger, so a big snapshot or a flood never makes one huge message.
pub const OUTPUT_FRAME_BYTES: usize = 64 * 1024;
/// The last output sent, kept for a client that comes back and only needs what it missed.
pub const OUTPUT_RING_BYTES: usize = 1024 * 1024;
/// A snapshot waits at most this long for a program to finish drawing a frame (mode 2026).
pub const SNAPSHOT_SYNC_WAIT_MS: u64 = 150;
/// A client with more output not acked than this is behind: its live output is dropped.
pub const CLIENT_HIGH_WATER: u64 = 512 * 1024;
/// A hidden client gets at most this much live output per window, counted from when it hid.
pub const HIDDEN_BUDGET_BYTES: u64 = 256 * 1024;
pub const HIDDEN_BUDGET_WINDOW_MS: u64 = 1000;
/// A behind client is caught up once the shell wrote less than cols * rows / 8 bytes in the last
/// RESYNC_QUIET_WINDOW_MS, or once it has been behind RESYNC_MAX_WAIT_MS. It gets at most one
/// snapshot per RESYNC_MAX_WAIT_MS.
pub const RESYNC_QUIET_WINDOW_MS: u64 = 100;
pub const RESYNC_MAX_WAIT_MS: u64 = 2000;
/// A behind client whose acks made no progress for this long, while the shell is quiet, gets a
/// snapshot anyway: a stuck tab or a client bug must not freeze its screen for good.
pub const ACK_STALL_MS: u64 = 5000;
/// A shell gets this long to end after SIGHUP, then it is killed.
pub const KILL_GRACE_MS: u64 = 2000;
/// How long a killed shell may take to end before a close stops waiting.
pub const EXIT_WAIT_MS: u64 = 1000;
/// A shell of a done or cancelled task that nobody looks at ends after this long, unless a
/// program runs in it.
pub const ENDED_IDLE_MS: u64 = 60_000;
/// The shells of a task that printed nothing for about this long went quiet.
pub const WORKSPACE_QUIET_MS: u64 = 500;
