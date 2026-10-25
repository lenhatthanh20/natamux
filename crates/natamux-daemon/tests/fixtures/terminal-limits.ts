import { SHELL_PROGRAMS } from '@natamux/core'

// A task of the default pipeline keeps three agent shells (1, 2 and 5), so ten tasks fit. Each
// stage with its own agent adds one more, from Shell 6, and none closes before the task ends.
export const MAX_SESSIONS = 30
export const TICKET_TTL_MS = 30_000
// Output is sent in batches. A chunk that comes after a quiet moment and is small (typing echo)
// goes out on the next turn; anything else waits this long for more.
export const OUTPUT_COALESCE_MS = 4
export const OUTPUT_IDLE_BYTES = 1024
// No frame is larger, so a big snapshot or a flood never makes one huge message.
export const OUTPUT_FRAME_BYTES = 64 * 1024
// The last output sent, kept for a client that comes back and only needs what it missed.
export const OUTPUT_RING_BYTES = 1024 * 1024
// A snapshot of a long history is serialized in slices of about this long, so other shells keep
// moving meanwhile.
export const SERIALIZE_SLICE_MS = 12
// Rows of the first slice, before the speed of this screen is known.
export const SNAPSHOT_FIRST_SLICE_ROWS = 250
export const SNAPSHOT_MIN_SLICE_ROWS = 50
// A snapshot waits at most this long for a program to finish drawing a frame (mode 2026).
export const SNAPSHOT_SYNC_WAIT_MS = 150
// A snapshot is sent at most this much per turn (or SERIALIZE_SLICE_MS of sending), so a large one
// does not block other shells either.
export const SNAPSHOT_SEND_BYTES = 256 * 1024
// Bytes the daemon's screen has not parsed yet, the freeze queue of a snapshot included. Above the
// high mark the shell is paused; it goes on below the low mark.
export const SCREEN_HIGH_WATER = 1_000_000
export const SCREEN_LOW_WATER = 250_000
// A client with more output not acked than this is behind: its live output is dropped.
export const CLIENT_HIGH_WATER = 512 * 1024
// A hidden client gets at most this much live output per window, counted from when it hid. Its
// tab parses all of it on the main thread; past the budget a resume or snapshot catches it up.
export const HIDDEN_BUDGET_BYTES = 256 * 1024
export const HIDDEN_BUDGET_WINDOW_MS = 1000
// A behind client is caught up once the shell wrote less than cols * rows / 8 bytes in the last
// RESYNC_QUIET_WINDOW_MS, or once it has been behind RESYNC_MAX_WAIT_MS. It gets at most one
// snapshot per RESYNC_MAX_WAIT_MS.
export const RESYNC_QUIET_WINDOW_MS = 100
export const RESYNC_MAX_WAIT_MS = 2000
// A behind client whose acks made no progress for this long, while the shell is quiet, gets a
// snapshot anyway: a stuck tab or a client bug must not freeze its screen for good.
export const ACK_STALL_MS = 5000
// A shell gets this long to end after SIGHUP, then it is killed.
export const KILL_GRACE_MS = 2000
// How long a killed shell may take to end before closeAndWait or closeAllAndWait stops waiting.
export const EXIT_WAIT_MS = 1000
// Time between a DEL and other input that a program must read apart, see TerminalService.write.
export const INPUT_GAP_MS = 25
// Time between a paste and its Enter, see TerminalService.paste.
export const ENTER_AFTER_PASTE_MS = 100

// A shell of a done or cancelled task that nobody looks at ends after this long, unless a program
// runs in it. A dashboard that comes back sooner finds it as it was.
export const ENDED_IDLE_MS = 60_000
// The shells of a task that printed nothing for about this long went quiet, see onOutputIdle.
export const WORKSPACE_QUIET_MS = 500

// A task can have several terminals, named "1", "2" and so on. The agent runs in the first.
export const MAIN_TERMINAL = '1'

export const SHELLS = SHELL_PROGRAMS
