import type { TerminalServerMessage, TerminalSize } from '@natamux/core'
import type { EventBus } from '../shared/event-bus.js'
import type { ScreenState } from './screen-state.js'

// The part of a pseudo terminal that the service uses. `node-pty` gives one in main.ts; tests give
// a fake, so they do not need the native module.
export type PtyProcess = {
  // UTF-8 bytes as the pty read them. A read can stop inside a character.
  onData: (listener: (data: Buffer) => void) => void
  onExit: (listener: (code: number | null) => void) => void
  write: (data: string) => void
  resize: (cols: number, rows: number) => void
  // Stop and go on reading output. Input still goes in while paused.
  pause: () => void
  resume: () => void
  // A signal name such as 'SIGHUP'. Without one, the default of the platform.
  kill: (signal?: string) => void
  // True when a process that the last SIGHUP reached still runs, though the shell may have ended.
  leftover?: () => boolean
  // The name of the program in the foreground, such as "bash" or "node".
  foreground?: () => string | undefined
  // The command lines of the processes under the shell, parents first, when the platform tells.
  commands?: () => Promise<string[] | undefined>
}

export type SpawnPty = (
  options: TerminalSize & { cwd: string; env: NodeJS.ProcessEnv },
) => PtyProcess

// A binary frame (output bytes for xterm) or a control message (sent as JSON text).
export type TerminalFrame = TerminalServerMessage | Uint8Array

export type TerminalClient = {
  send: (frame: TerminalFrame) => void
}

export class NoTerminalError extends Error {
  constructor(taskId: string) {
    super(`${taskId} has no open terminal. Start the agent first`)
    this.name = 'NoTerminalError'
  }
}

// A resume names a shell by its id. Thrown when no shell of that name runs, or another one does.
export class ShellGoneError extends Error {
  constructor(taskId: string, terminal: string) {
    super(`Shell ${terminal} of ${taskId} has ended`)
    this.name = 'ShellGoneError'
  }
}

export class TerminalLimitError extends Error {
  constructor(limit: number) {
    super(`At most ${limit} terminals can be open. Close one first.`)
    this.name = 'TerminalLimitError'
  }
}

// The size of a shell started before any dashboard looks at it. The first resize fixes it.
export const DEFAULT_SIZE: TerminalSize = { cols: 120, rows: 32 }

export type ClientState = {
  // The size of the client's screen.
  size: TerminalSize
  // Set once it claimed the size. Only such clients give the shell their size back.
  claimed: boolean
  // Its last `visible` message, or the `visible` query.
  visible: boolean
  // 'snapshot' until its replayed is sent. Before the cut its output is dropped (the snapshot has
  // it); from the cut on it waits in `waiting`. 'behind' while its live output is dropped.
  phase: 'snapshot' | 'live' | 'behind'
  waiting: Buffer[] | null
  waitingBytes: number
  // Offsets: the end of the output sent to it, and of the output it acked. `ackedAt` is when its
  // acks last moved on (or it was caught up).
  sent: number
  acked: number
  ackedAt: number
  // The first byte it missed, and when. Also set in the snapshot phase once `waiting` passed its
  // cap: the client then gets the snapshot alone and is behind from the cut.
  behindFrom: number | null
  behindSince: number
  // When it last asked for a snapshot.
  snapshotAt: number
  // The window of the hidden budget it is in, and the live output it got in it.
  budgetFrom: number
  budgetBytes: number
}

export type SnapshotJob = {
  clients: Set<TerminalClient>
  // The offset of the cut, or -1 while it waits for a frame of a program to end.
  cut: number
  // The shell size at the cut.
  size: TerminalSize
  // Set once its frames are being sent: a client that comes then would miss the first ones.
  sending: boolean
  done: Promise<void>
  finish: () => void
}

export type Session = {
  taskId: string
  terminal: string
  // Random, set when the shell starts, so a client can tell a new shell from the one it had.
  shellId: string
  pty: PtyProcess
  screen: ScreenState
  // The client used last is the last one in the map.
  clients: Map<TerminalClient, ClientState>
  size: TerminalSize
  // Bytes of all output frames sent so far.
  offset: number
  // Output not sent yet, see queueOutput().
  batch: Buffer[]
  batchBytes: number
  lastFlushAt: number
  flushTimer?: NodeJS.Timeout
  flushImmediate?: NodeJS.Immediate
  // The last frames sent, at most OUTPUT_RING_BYTES (or one frame), from offset `ringStart`.
  ring: Buffer[]
  ringBytes: number
  ringStart: number
  // The snapshot that runs, and the clients that wait for the next one.
  snapshot?: SnapshotJob
  nextSnapshot: Set<TerminalClient>
  // Set while the pty is paused because the screen is too far behind.
  paused: boolean
  // Output of the last RESYNC_QUIET_WINDOW_MS, to tell when the shell is quiet.
  recent: { at: number; bytes: number }[]
  recentBytes: number
  // The next look at the clients that are behind.
  resyncTimer?: NodeJS.Timeout
  // Set when the shell is being closed: the pending SIGKILL is not needed once it has ended.
  stopKillTimer?: () => void
  // Set when the shell is being closed: ends when the SIGKILL went out or is not needed.
  killDone?: Promise<void>
  // Set while the shell of an ended task waits, with no client, to be closed.
  idleTimer?: NodeJS.Timeout
  // Input that waits for its turn, see write().
  pending: Input[]
  lastWrite?: { at: number; kind: Input['kind'] }
  // The first bytes of a character that the last read cut, see wholeChars.
  partial?: Buffer
  writeTimer?: NodeJS.Timeout
  // The program in the foreground when the output last went quiet, see noteForeground.
  program?: string
}

// A piece of input for a shell. Pieces of another kind than the one before go a moment apart.
type Input = { data: string; kind: 'text' | 'erase' | 'submit' }

export type TerminalServiceOptions = {
  spawn: SpawnPty
  // The folder of the task's workspace. Throws when the task has none.
  workspaceOf: (taskId: string) => string
  env?: NodeJS.ProcessEnv
  now?: () => number
  // Called with every piece of output, for signs of life of an agent without hooks.
  onOutput?: (taskId: string, terminal: string, data: Buffer) => void
  // True when the task is done or cancelled, so its shells are only there to look around.
  isEnded?: (taskId: string) => boolean
  // Dashboards hear when a shell starts or ends.
  bus?: EventBus
  // Called once the shells of a task went quiet after output: files in the workspace may have
  // changed.
  onOutputIdle?: (taskId: string) => void
}

export type Ticket = { taskId: string; expiresAt: number }
