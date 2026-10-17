import { randomBytes, timingSafeEqual } from 'node:crypto'
import { basename } from 'node:path'
import {
  ACK_EVERY_BYTES,
  plainText,
  portRange,
  SHELL_PROGRAMS,
  scrollbackLines,
  TERMINAL_SCROLLBACK_MAX_LINES,
  type TerminalServerMessage,
  type TerminalSize,
} from '@natamux/core'
import { SerializeAddon } from '@xterm/addon-serialize'
import { Unicode11Addon } from '@xterm/addon-unicode11'
// The package is CommonJS without named exports for Node.js.
import xtermHeadless from '@xterm/headless'
import type { EventBus } from './event-bus.js'
import { withLoopbackNoProxy } from './no-proxy.js'

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
// The size of a shell started before any dashboard looks at it. The first resize fixes it.
const DEFAULT_SIZE: TerminalSize = { cols: 120, rows: 32 }

// A shell of a done or cancelled task that nobody looks at ends after this long, unless a program
// runs in it. A dashboard that comes back sooner finds it as it was.
export const ENDED_IDLE_MS = 60_000
// The shells of a task that printed nothing for about this long went quiet, see onOutputIdle.
export const WORKSPACE_QUIET_MS = 500

// A task can have several terminals, named "1", "2" and so on. The agent runs in the first.
export const MAIN_TERMINAL = '1'

export const SHELLS = SHELL_PROGRAMS

// What a snapshot of the screen asks its owner to do, see ScreenState.snapshot.
export type SnapshotSteps = {
  // The snapshot holds the output up to now: send what is pending, and note the offset.
  cut: () => void
  // The screen went on parsing to let a frame of a program end, so the last cut is no longer valid.
  uncut: () => void
  // The serialized parts, in order. Not joined: a copy of megabytes would block a turn.
  done: (parts: Buffer[], size: TerminalSize) => void
}

type Queued = { data: Buffer } | { run: () => void }

// The screen of a shell, kept the way a terminal shows it. Programs such as agents place text
// with cursor moves for the size the shell had at that moment, so the raw output shows garbage
// on a screen of another size. A dashboard that connects gets this screen instead, drawn again
// for its size, with the lines above it.
export class ScreenState {
  private readonly terminal: InstanceType<typeof xtermHeadless.Terminal>
  private readonly serializer = new SerializeAddon()
  private readonly now: () => number
  // Output parses later; what comes after the shell is gone must not touch the screen.
  private disposed = false
  // A snapshot reads the buffer over several turns, so it must not change meanwhile. Writes and
  // resizes wait here, in order, until the snapshot is done.
  private frozen = false
  private queue: Queued[] = []
  // Rows serialized per turn, learned from the last turn so one takes about SERIALIZE_SLICE_MS.
  private sliceRows = SNAPSHOT_FIRST_SLICE_ROWS
  // Bytes written but not parsed yet, the queue included, see SCREEN_HIGH_WATER.
  private unparsed = 0
  private full = false
  private readonly onFull: (full: boolean) => void

  // onFull(true) when the screen falls too far behind, onFull(false) when it has caught up.
  constructor(
    size: TerminalSize,
    now: () => number = Date.now,
    onFull: (full: boolean) => void = () => undefined,
  ) {
    this.now = now
    this.onFull = onFull
    this.terminal = new xtermHeadless.Terminal({
      ...size,
      scrollback: scrollbackLines(size.cols),
      allowProposedApi: true,
    })
    this.terminal.loadAddon(this.serializer)
    // The same widths as the dashboard (TerminalView.tsx), so an emoji takes two cells in both.
    this.terminal.loadAddon(new Unicode11Addon())
    this.terminal.unicode.activeVersion = '11'
  }

  write(data: Buffer): void {
    if (this.disposed) {
      return
    }
    this.count(data.length)
    if (this.frozen) {
      this.queue.push({ data })
    } else {
      this.feed(data)
    }
  }

  // Output is parsed later, in order. A resize waits for the output before it, which was drawn
  // for the old size.
  // The history keeps the same memory at the new width, as the dashboard's does.
  resize(size: TerminalSize): void {
    this.whenParsed(() => {
      this.terminal.resize(size.cols, size.rows)
      this.terminal.options.scrollback = scrollbackLines(size.cols)
    })
  }

  // Serializes the screen and the lines above it, exactly as they were at the cut. The cut is
  // now, unless the screen is inside a frame that a program draws at once (mode 2026): then it
  // waits for the frame to end, up to SNAPSHOT_SYNC_WAIT_MS, and cuts again.
  snapshot(steps: SnapshotSteps): void {
    const deadline = this.now() + SNAPSHOT_SYNC_WAIT_MS
    const attempt = () => {
      if (this.disposed) {
        return
      }
      steps.cut()
      this.frozen = true
      // Straight to xterm, not to the queue: it runs once the output before the cut is parsed.
      this.terminal.write('', drained)
    }
    const drained = () => {
      if (this.disposed) {
        return
      }
      if (this.terminal.modes.synchronizedOutputMode && this.now() < deadline) {
        // The screen must go on parsing, or the frame never ends.
        steps.uncut()
        this.thaw()
        this.waitForFrameEnd(deadline, attempt)
        return
      }
      this.serializeSliced((parts) => {
        const size = { cols: this.terminal.cols, rows: this.terminal.rows }
        // Before done, which may start the next snapshot and freeze again.
        this.thaw()
        steps.done(parts, size)
      })
    }
    attempt()
  }

  // Resolves once all output so far is on the screen.
  settled(): Promise<void> {
    return new Promise((resolve) => (this.disposed ? resolve() : this.whenParsed(resolve)))
  }

  // The last lines of the screen and the history above it, as plain text, without the empty rows
  // under the last output. Only the output parsed so far counts, see settled().
  lastLines(max: number): string {
    if (this.disposed) {
      return ''
    }
    const buffer = this.terminal.buffer.active
    const line = (row: number) => buffer.getLine(row)?.translateToString(true) ?? ''
    let end = buffer.length
    while (end > 0 && line(end - 1) === '') {
      end -= 1
    }
    const lines: string[] = []
    for (let row = Math.max(0, end - max); row < end; row += 1) {
      lines.push(line(row))
    }
    return lines.join('\n')
  }

  dispose(): void {
    this.disposed = true
    this.queue = []
    this.terminal.dispose()
  }

  private feed(data: Buffer): void {
    this.terminal.write(data, () => {
      if (!this.disposed) {
        this.count(-data.length)
      }
    })
  }

  // Two marks, so a backlog near one of them does not pause and resume the shell over and over.
  private count(bytes: number): void {
    this.unparsed += bytes
    if (!this.full && this.unparsed > SCREEN_HIGH_WATER) {
      this.full = true
      this.onFull(true)
    } else if (this.full && this.unparsed < SCREEN_LOW_WATER) {
      this.full = false
      this.onFull(false)
    }
  }

  private whenParsed(run: () => void): void {
    if (this.disposed) {
      return
    }
    if (this.frozen) {
      this.queue.push({ run })
      return
    }
    this.terminal.write('', () => {
      if (!this.disposed) {
        run()
      }
    })
  }

  // xterm runs a write callback right after its chunk, so a queued resize lands between the same
  // writes as it would have without the freeze.
  private thaw(): void {
    this.frozen = false
    const queue = this.queue
    this.queue = []
    for (const item of queue) {
      if ('data' in item) {
        this.feed(item.data)
      } else {
        this.whenParsed(item.run)
      }
    }
  }

  private waitForFrameEnd(deadline: number, retry: () => void): void {
    let waiting = true
    const stop = () => {
      if (waiting) {
        waiting = false
        parsed.dispose()
        clearTimeout(timer)
        retry()
      }
    }
    const parsed = this.terminal.onWriteParsed(() => {
      if (!this.terminal.modes.synchronizedOutputMode) {
        stop()
      }
    })
    const timer = setTimeout(stop, Math.max(0, deadline - this.now()))
  }

  // A short history is serialized in one call. A long one is serialized a slice of rows at a
  // time, a turn of SERIALIZE_SLICE_MS each, and the last part adds the screen, the cursor, the
  // alternate screen and the modes.
  private serializeSliced(done: (parts: Buffer[]) => void): void {
    const buffer = this.terminal.buffer.normal
    const rows = this.terminal.rows
    // The history rows: `serialize({ scrollback })` counts back from the end of the buffer.
    const history = buffer.length - rows
    const wrapped = (row: number) => buffer.getLine(row)?.isWrapped === true
    // The last part starts at a row that is not the rest of a wrapped line.
    let k = 0
    while (k < history && wrapped(history - k)) {
      k += 1
    }
    if (history <= this.sliceRows || wrapped(history - k)) {
      done([Buffer.from(this.serializer.serialize())])
      return
    }
    const historyEnd = history - k
    const parts: Buffer[] = []
    let start = 0
    const turn = () => {
      if (this.disposed) {
        return
      }
      const began = performance.now()
      const first = start
      const stop = Math.min(start + this.sliceRows, historyEnd)
      let text = ''
      // Each piece fits on one screen, see historyJoin. The turn also ends once it took
      // SERIALIZE_SLICE_MS, when rows cost more than sliceRows assumed (a cold first turn).
      while (start < stop && (start === first || performance.now() - began < SERIALIZE_SLICE_MS)) {
        const end = sliceEnd(wrapped, start, historyEnd, rows)
        const range = { start, end: end - 1 }
        text += this.serializer.serialize({ range, excludeAltBuffer: true, excludeModes: true })
        text += historyJoin(end - start, rows)
        start = end
      }
      // Encoded now, so no single large Buffer.from blocks a turn at the end.
      parts.push(Buffer.from(text))
      const fit = ((start - first) / Math.max(performance.now() - began, 0.1)) * SERIALIZE_SLICE_MS
      this.sliceRows = Math.min(
        TERMINAL_SCROLLBACK_MAX_LINES,
        Math.max(SNAPSHOT_MIN_SLICE_ROWS, Math.floor(fit)),
      )
      setImmediate(start < historyEnd ? turn : last)
    }
    const last = () => {
      if (this.disposed) {
        return
      }
      parts.push(Buffer.from(this.serializer.serialize({ scrollback: k })))
      done(parts)
    }
    turn()
  }
}

// The serializer writes each piece as if from the top of an empty screen. Past the first screen,
// a terminal scrolls, and fills the new row with the background of the current style; the
// serializer expects that only from the second screen of a piece on. So each history piece fits
// on one screen, and between pieces the style is reset, the piece's rows are scrolled into the
// history, and the cursor goes back to the top of the now empty screen.
function historyJoin(count: number, rows: number): string {
  return `\x1b[0m\x1b[${rows};1H${'\n'.repeat(Math.min(count, rows))}\x1b[H`
}

// The exclusive end of the slice that begins at `start`: at most `rows` rows, before a row that
// is not the rest of a wrapped line, so no line is cut. A wrapped line longer than `rows` stays
// whole.
export function sliceEnd(
  isWrapped: (row: number) => boolean,
  start: number,
  end: number,
  rows: number,
): number {
  const stop = Math.min(start + rows, end)
  let back = stop
  while (back > start && back < end && isWrapped(back)) {
    back -= 1
  }
  if (back > start) {
    return back
  }
  let forward = stop
  while (forward < end && isWrapped(forward)) {
    forward += 1
  }
  return forward
}

// `bytes` in frames of at most `max` bytes, none of them ending inside a UTF-8 character.
export function splitFrames(bytes: Buffer, max = OUTPUT_FRAME_BYTES): Buffer[] {
  const frames: Buffer[] = []
  let start = 0
  while (start < bytes.length) {
    const end = charBorder(bytes, start, Math.min(start + max, bytes.length))
    frames.push(bytes.subarray(start, end))
    start = end
  }
  return frames
}

// `end`, moved back to the start of the UTF-8 character it falls in. Never back to `start`, so a
// piece is never empty.
function charBorder(bytes: Buffer, start: number, end: number): number {
  let border = end
  while (border < bytes.length && border > start && ((bytes[border] ?? 0) & 0xc0) === 0x80) {
    border -= 1
  }
  return border > start ? border : end
}

// The end of the last whole UTF-8 character. The screen would join a cut character by itself, but
// a frame, a snapshot cut or a listener's text must not split one.
function wholeChars(bytes: Buffer): number {
  for (let back = 1; back <= Math.min(4, bytes.length); back += 1) {
    const byte = bytes[bytes.length - back] ?? 0
    if ((byte & 0xc0) !== 0x80) {
      const size = byte >= 0xf0 ? 4 : byte >= 0xe0 ? 3 : byte >= 0xc0 ? 2 : 1
      return size > back ? bytes.length - back : bytes.length
    }
  }
  return bytes.length
}

function sameSize(a: TerminalSize, b: TerminalSize): boolean {
  return a.cols === b.cols && a.rows === b.rows
}

function sessionKey(taskId: string, terminal: string): string {
  return `${taskId}/${terminal}`
}

type ClientState = {
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

type SnapshotJob = {
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

type Session = {
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

function exitOf(session: Session): Promise<void> {
  return new Promise((resolve) => session.pty.onExit(() => resolve()))
}

type TerminalServiceOptions = {
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

type Ticket = { taskId: string; expiresAt: number }

// Shells in the workspace of a task. A shell keeps running when the dashboard closes, so an agent
// started in it goes on; the next dashboard that connects gets the screen so far first.
// One shell has one size, and several dashboards can show it. The one used last decides it.
export class TerminalService {
  private readonly spawn: SpawnPty
  private readonly workspaceOf: (taskId: string) => string
  private readonly env: NodeJS.ProcessEnv
  private readonly now: () => number
  private readonly onOutput: (taskId: string, terminal: string, data: Buffer) => void
  private readonly isEnded: (taskId: string) => boolean
  private readonly sessions = new Map<string, Session>()
  private readonly tickets = new Map<string, Ticket>()
  private readonly bus: EventBus | undefined
  // By task: whether output came since the quiet timer was set.
  private readonly outputSeen = new Map<string, { seen: boolean }>()

  constructor(options: TerminalServiceOptions) {
    this.spawn = options.spawn
    this.workspaceOf = options.workspaceOf
    this.env = withLoopbackNoProxy(options.env ?? process.env)
    this.now = options.now ?? Date.now
    const onOutput = options.onOutput ?? (() => undefined)
    const onOutputIdle = options.onOutputIdle
    this.onOutput = (taskId, terminal, data) => {
      if (onOutputIdle !== undefined) {
        this.noteOutput(taskId, onOutputIdle)
      }
      onOutput(taskId, terminal, data)
    }
    this.isEnded = options.isEnded ?? (() => false)
    this.bus = options.bus
  }

  // A flag per task, not a new timer per chunk, so a flood stays cheap. The quiet moment is
  // between WORKSPACE_QUIET_MS and twice that.
  private noteOutput(taskId: string, onQuiet: (taskId: string) => void): void {
    const known = this.outputSeen.get(taskId)
    if (known !== undefined) {
      known.seen = true
      return
    }
    const entry = { seen: false }
    this.outputSeen.set(taskId, entry)
    const check = () => {
      if (entry.seen) {
        entry.seen = false
        setTimeout(check, WORKSPACE_QUIET_MS).unref()
        return
      }
      this.outputSeen.delete(taskId)
      try {
        onQuiet(taskId)
      } catch {
        // A failing listener must not break the terminal.
      }
      this.noteForeground(taskId)
    }
    setTimeout(check, WORKSPACE_QUIET_MS).unref()
  }

  // A program that starts or ends in a shell prints something. The dashboards show it, and an
  // agent started by hand, with the shells: tell them when it changed.
  private noteForeground(taskId: string): void {
    let changed = false
    for (const session of this.sessions.values()) {
      if (session.taskId === taskId) {
        const program = session.pty.foreground?.()
        changed = changed || program !== session.program
        session.program = program
      }
    }
    if (changed) {
      this.bus?.publish({ kind: 'shells.changed', taskId })
    }
  }

  // A browser cannot set headers on a WebSocket. Instead of the daemon token in the URL, it asks
  // for a ticket with the token, then opens the socket with the ticket. A ticket works once.
  issueTicket(taskId: string): string {
    this.workspaceOf(taskId)
    this.dropExpiredTickets()
    const ticket = randomBytes(32).toString('base64url')
    this.tickets.set(ticket, { taskId, expiresAt: this.now() + TICKET_TTL_MS })
    return ticket
  }

  redeemTicket(ticket: string, taskId: string): boolean {
    this.dropExpiredTickets()
    for (const [stored, entry] of this.tickets) {
      if (sameText(stored, ticket)) {
        this.tickets.delete(stored)
        return entry.taskId === taskId
      }
    }
    return false
  }

  isOpen(taskId: string, terminal = MAIN_TERMINAL): boolean {
    return this.sessions.has(sessionKey(taskId, terminal))
  }

  // The names of the open terminals of a task, in order.
  list(taskId: string): string[] {
    return [...this.sessions.values()]
      .filter((session) => session.taskId === taskId)
      .map((session) => session.terminal)
      .sort((a, b) => Number(a) - Number(b))
  }

  // Starts the shell when there is none. Returns the function that detaches the client. A client
  // that is not on screen does not take the shell's size.
  // With `shellId` it is a resume of a client that lost its socket: it never starts a shell, never
  // takes the size, and gets only the output since `offset` while the ring still has it.
  attach(
    taskId: string,
    client: TerminalClient,
    size: TerminalSize,
    terminal = MAIN_TERMINAL,
    options: { visible?: boolean; shellId?: string; offset?: number } = {},
  ): () => void {
    const visible = options.visible ?? true
    const key = sessionKey(taskId, terminal)
    const existed = this.sessions.has(key)
    const resumed = options.shellId === undefined ? undefined : this.sessions.get(key)
    if (options.shellId !== undefined && resumed?.shellId !== options.shellId) {
      throw new ShellGoneError(taskId, terminal)
    }
    const session = resumed ?? this.ensure(taskId, terminal, size)
    const claims = visible && resumed === undefined
    const now = this.now()
    const state: ClientState = {
      size,
      claimed: claims,
      visible,
      phase: 'snapshot',
      waiting: null,
      waitingBytes: 0,
      sent: session.offset,
      acked: session.offset,
      ackedAt: now,
      behindFrom: null,
      behindSince: now,
      snapshotAt: now,
      budgetFrom: now,
      budgetBytes: 0,
    }
    if (claims) {
      session.clients.set(client, state)
      if (existed) {
        // The screen is drawn for this size, so the shell must know it before the snapshot.
        this.setSize(session, size)
      }
    } else {
      // First, as the one used least: another client leaving never gives the shell its size.
      session.clients = new Map<TerminalClient, ClientState>([[client, state], ...session.clients])
    }
    clearTimeout(session.idleTimer)
    // No output reaches it before: it is in the snapshot phase with nothing waiting.
    client.send({ type: 'hello', shellId: session.shellId })
    if (resumed === undefined || !this.resume(session, client, state, options.offset)) {
      this.requestSnapshot(session, client, state)
    }
    return () => {
      if (session.clients.get(client) !== state) {
        return
      }
      session.clients.delete(client)
      session.snapshot?.clients.delete(client)
      session.nextSnapshot.delete(client)
      // The client that claimed the size last decides it again.
      const last = [...session.clients.values()].filter((other) => other.claimed).at(-1)
      if (last) {
        this.setSize(session, last.size)
      } else if (session.clients.size === 0) {
        this.closeWhenIdle(session)
      }
    }
  }

  // A client came on screen or left it. Only a terminal that came on screen in the page sends its
  // size, and only that claims the shell's size: a page that comes back to the front does not.
  visible(
    taskId: string,
    message: { visible: boolean; cols?: number; rows?: number },
    terminal = MAIN_TERMINAL,
    client?: TerminalClient,
  ): void {
    const session = this.sessions.get(sessionKey(taskId, terminal))
    const state = client ? session?.clients.get(client) : undefined
    if (!session || !state || !client) {
      return
    }
    if (state.visible && !message.visible) {
      state.budgetFrom = this.now()
      state.budgetBytes = 0
    }
    state.visible = message.visible
    if (message.visible && message.cols !== undefined && message.rows !== undefined) {
      this.touch(session, client)
      state.size = { cols: message.cols, rows: message.rows }
      this.setSize(session, state.size)
    }
    if (message.visible && state.phase === 'behind') {
      this.catchUpBehind(session)
    }
  }

  // The client parsed its output up to `offset`. An offset past what it was sent counts as all of
  // it, and an older one changes nothing.
  ack(taskId: string, offset: number, terminal = MAIN_TERMINAL, client?: TerminalClient): void {
    const session = this.sessions.get(sessionKey(taskId, terminal))
    const state = client ? session?.clients.get(client) : undefined
    if (!session || !state) {
      return
    }
    const acked = Math.min(Math.max(state.acked, offset), state.sent)
    if (acked > state.acked) {
      state.acked = acked
      state.ackedAt = this.now()
    }
    if (state.phase === 'behind') {
      this.catchUpBehind(session)
    }
  }

  // The output sent since `offset`, while it is still kept. Undefined when it is not (too old,
  // in the future, or inside a frame).
  outputSince(taskId: string, offset: number, terminal = MAIN_TERMINAL): Buffer[] | undefined {
    const session = this.sessions.get(sessionKey(taskId, terminal))
    return session ? this.ringSince(session, offset) : undefined
  }

  private ringSince(session: Session, offset: number): Buffer[] | undefined {
    if (offset < session.ringStart || offset > session.offset) {
      return undefined
    }
    let at = session.ringStart
    for (const [index, frame] of session.ring.entries()) {
      if (at === offset) {
        return session.ring.slice(index)
      }
      if (at > offset) {
        return undefined
      }
      at += frame.length
    }
    return at === offset ? [] : undefined
  }

  // Sends a client that comes back the output since `offset`, when the ring still has all of it.
  // False when it does not (too old, in the future, inside a frame, or no offset): the client then
  // needs a snapshot.
  private resume(
    session: Session,
    client: TerminalClient,
    state: ClientState,
    offset: number | undefined,
  ): boolean {
    // The client is in the snapshot phase with nothing waiting, so the flush skips it and the ring
    // gets the pending output.
    this.flushOutput(session)
    const missed = offset === undefined ? undefined : this.ringSince(session, offset)
    if (offset === undefined || missed === undefined) {
      return false
    }
    client.send({ type: 'resume', offset })
    for (const frame of missed) {
      client.send(frame)
    }
    client.send({ type: 'replayed', offset: session.offset })
    state.sent = session.offset
    state.acked = session.offset
    state.phase = 'live'
    return true
  }

  // A new client gets the running snapshot when its cut is right for it too: no output since the
  // cut, the same size, and no frame sent yet. Otherwise it takes the next one, which all such
  // clients share.
  private requestSnapshot(session: Session, client: TerminalClient, state: ClientState): void {
    const job = session.snapshot
    if (job === undefined) {
      this.startSnapshot(session, new Set([client]))
      return
    }
    const sameCut =
      job.cut === session.offset && session.batchBytes === 0 && sameSize(job.size, session.size)
    if (!job.sending && (job.cut === -1 || sameCut)) {
      job.clients.add(client)
      if (job.cut !== -1) {
        state.waiting = []
      }
    } else {
      // Its output is dropped until the next cut; that snapshot has it.
      session.nextSnapshot.add(client)
    }
  }

  private startSnapshot(session: Session, clients: Set<TerminalClient>): void {
    let finish = (): void => undefined
    const done = new Promise<void>((resolve) => {
      finish = () => resolve()
    })
    const job: SnapshotJob = { clients, cut: -1, size: session.size, sending: false, done, finish }
    session.snapshot = job
    session.screen.snapshot({
      cut: () => {
        this.flushOutput(session)
        job.cut = session.offset
        job.size = session.size
        // Output from here on is not in the snapshot.
        for (const client of job.clients) {
          const state = session.clients.get(client)
          if (state) {
            state.waiting = []
            state.waitingBytes = 0
            state.behindFrom = null
          }
        }
      },
      uncut: () => {
        job.cut = -1
      },
      done: (parts, size) => this.sendSnapshot(session, job, parts, size),
    })
  }

  private sendSnapshot(
    session: Session,
    job: SnapshotJob,
    parts: Buffer[],
    size: TerminalSize,
  ): void {
    if (session.snapshot !== job) {
      return
    }
    // Over several turns. The clients stay in the snapshot phase meanwhile, so new output waits in
    // their `waiting` lists, and a client that leaves is no longer in the job.
    job.sending = true
    this.flushOutput(session)
    for (const client of job.clients) {
      client.send({ type: 'snapshot', cols: size.cols, rows: size.rows })
    }
    const frames = parts.flatMap((part) => splitFrames(part))
    const sendTurn = () => {
      if (session.snapshot !== job) {
        return
      }
      const began = performance.now()
      let sent = 0
      while (sent < SNAPSHOT_SEND_BYTES && performance.now() - began < SERIALIZE_SLICE_MS) {
        const frame = frames.shift()
        if (frame === undefined) {
          break
        }
        sent += frame.length
        for (const client of job.clients) {
          client.send(frame)
        }
      }
      if (frames.length > 0) {
        setImmediate(sendTurn)
      } else {
        this.endSnapshot(session, job)
      }
    }
    sendTurn()
  }

  private endSnapshot(session: Session, job: SnapshotJob): void {
    session.snapshot = undefined
    // Output pending now goes to the waiting lists, so replayed covers it.
    this.flushOutput(session)
    for (const client of job.clients) {
      const state = session.clients.get(client)
      if (!state) {
        continue
      }
      for (const frame of state.waiting ?? []) {
        client.send(frame)
      }
      const offset = state.behindFrom ?? session.offset
      client.send({ type: 'replayed', offset })
      state.waiting = null
      state.waitingBytes = 0
      // A catch-up does not count as unacked: the client acks its end once it is parsed.
      state.sent = offset
      state.acked = offset
      state.ackedAt = this.now()
      state.phase = state.behindFrom === null ? 'live' : 'behind'
    }
    job.finish()
    if (session.nextSnapshot.size > 0) {
      const next = new Set(session.nextSnapshot)
      session.nextSnapshot.clear()
      this.startSnapshot(session, next)
    }
    this.catchUpBehind(session)
  }

  // Catches up the clients that are behind once they can take it (spec 4.3): they acked what they
  // got, they are visible, and the shell is quiet or they waited RESYNC_MAX_WAIT_MS. From the ring
  // when it still has what they missed, else with a snapshot. Otherwise it looks again at the
  // earliest time one of them could be ready.
  // A snapshot for a shell that still floods comes at most once per RESYNC_MAX_WAIT_MS, so it
  // cannot loop. A quiet shell cannot loop, so its snapshot goes at once: the prompt after a
  // Ctrl+C shows without waiting.
  private catchUpBehind(session: Session): void {
    clearTimeout(session.resyncTimer)
    session.resyncTimer = undefined
    const now = this.now()
    let next = Number.POSITIVE_INFINITY
    for (const [client, state] of session.clients) {
      if (state.phase !== 'behind' || state.behindFrom === null || !state.visible) {
        continue
      }
      const quietAt = this.quietAt(session, now)
      // The dashboard acks every ACK_EVERY_BYTES, so a smaller rest is all it can have parsed.
      if (state.sent - state.acked >= ACK_EVERY_BYTES) {
        // Its acks stopped: its offset cannot be trusted, so only a snapshot fixes it.
        const stalled = Math.max(
          quietAt,
          state.ackedAt + ACK_STALL_MS,
          state.snapshotAt + RESYNC_MAX_WAIT_MS,
        )
        if (stalled > now) {
          next = Math.min(next, stalled)
        } else {
          this.snapshotCatchUp(session, client, state, now)
        }
        continue
      }
      const ready = Math.min(state.behindSince + RESYNC_MAX_WAIT_MS, quietAt)
      if (ready > now) {
        next = Math.min(next, ready)
        continue
      }
      this.flushOutput(session)
      const missed = this.ringSince(session, state.behindFrom)
      if (missed) {
        client.send({ type: 'resume', offset: state.behindFrom })
        for (const frame of missed) {
          client.send(frame)
        }
        client.send({ type: 'replayed', offset: session.offset })
        state.sent = session.offset
        state.acked = session.offset
        state.ackedAt = now
        state.behindFrom = null
        state.phase = 'live'
      } else if (quietAt <= now || now >= state.snapshotAt + RESYNC_MAX_WAIT_MS) {
        this.snapshotCatchUp(session, client, state, now)
      } else {
        next = Math.min(next, quietAt, state.snapshotAt + RESYNC_MAX_WAIT_MS)
      }
    }
    if (next < Number.POSITIVE_INFINITY) {
      session.resyncTimer = setTimeout(() => this.catchUpBehind(session), next - now)
    }
  }

  private snapshotCatchUp(
    session: Session,
    client: TerminalClient,
    state: ClientState,
    now: number,
  ): void {
    state.behindFrom = null
    state.phase = 'snapshot'
    state.snapshotAt = now
    this.requestSnapshot(session, client, state)
  }

  // When the shell counts as quiet if it prints nothing more: less than cols * rows / 8 bytes in
  // the last RESYNC_QUIET_WINDOW_MS.
  private quietAt(session: Session, now: number): number {
    this.dropOldOutput(session, now)
    const limit = (session.size.cols * session.size.rows) / 8
    let left = session.recentBytes
    let at = now
    for (const entry of session.recent) {
      if (left < limit) {
        break
      }
      left -= entry.bytes
      at = entry.at + RESYNC_QUIET_WINDOW_MS
    }
    return at
  }

  private dropOldOutput(session: Session, now: number): void {
    while ((session.recent[0]?.at ?? now) <= now - RESYNC_QUIET_WINDOW_MS) {
      session.recentBytes -= session.recent.shift()?.bytes ?? 0
    }
  }

  private endSnapshots(session: Session): void {
    session.snapshot?.finish()
    session.snapshot = undefined
    session.nextSnapshot.clear()
  }

  // Output goes out in batches: one frame for many small chunks, see OUTPUT_COALESCE_MS.
  private queueOutput(session: Session, bytes: Buffer): void {
    if (bytes.length === 0) {
      return
    }
    const now = this.now()
    session.recent.push({ at: now, bytes: bytes.length })
    session.recentBytes += bytes.length
    this.dropOldOutput(session, now)
    session.batch.push(bytes)
    session.batchBytes += bytes.length
    if (session.batchBytes >= OUTPUT_FRAME_BYTES) {
      this.sendFullFrames(session)
    }
    if (
      session.batchBytes === 0 ||
      session.flushTimer !== undefined ||
      session.flushImmediate !== undefined
    ) {
      return
    }
    const idle = now - session.lastFlushAt >= OUTPUT_COALESCE_MS
    if (idle && session.batchBytes < OUTPUT_IDLE_BYTES) {
      session.flushImmediate = setImmediate(() => this.flushOutput(session))
    } else {
      session.flushTimer = setTimeout(() => this.flushOutput(session), OUTPUT_COALESCE_MS)
    }
  }

  // Sends the full frames of the batch and keeps the rest. A planned flush stays planned for it.
  private sendFullFrames(session: Session): void {
    const all = Buffer.concat(session.batch, session.batchBytes)
    let start = 0
    while (all.length - start >= OUTPUT_FRAME_BYTES) {
      const end = charBorder(all, start, start + OUTPUT_FRAME_BYTES)
      this.sendFrame(session, all.subarray(start, end))
      start = end
    }
    session.batch = start < all.length ? [all.subarray(start)] : []
    session.batchBytes = all.length - start
    session.lastFlushAt = this.now()
  }

  // Also called before every control message and pty resize, so output comes before them.
  private flushOutput(session: Session): void {
    clearTimeout(session.flushTimer)
    clearImmediate(session.flushImmediate)
    session.flushTimer = undefined
    session.flushImmediate = undefined
    if (session.batchBytes === 0) {
      return
    }
    const bytes = Buffer.concat(session.batch, session.batchBytes)
    session.batch = []
    session.batchBytes = 0
    session.lastFlushAt = this.now()
    for (const frame of splitFrames(bytes)) {
      this.sendFrame(session, frame)
    }
  }

  private sendFrame(session: Session, frame: Buffer): void {
    const start = session.offset
    session.offset += frame.length
    session.ring.push(frame)
    session.ringBytes += frame.length
    while (session.ringBytes > OUTPUT_RING_BYTES && session.ring.length > 1) {
      const dropped = session.ring.shift()
      session.ringBytes -= dropped?.length ?? 0
      session.ringStart += dropped?.length ?? 0
    }
    for (const [client, state] of session.clients) {
      if (state.phase === 'live') {
        if (this.admits(state, frame.length)) {
          client.send(frame)
          state.sent = session.offset
        } else {
          // Dropped, not queued, so a slow or hidden client costs no memory.
          state.phase = 'behind'
          state.behindFrom = start
          state.behindSince = this.now()
          // A look soon, not now (this runs inside a flush): one whose acks stop needs a timer.
          session.resyncTimer ??= setTimeout(() => this.catchUpBehind(session), 0)
        }
      } else if (state.waiting !== null) {
        if (state.waitingBytes + frame.length > CLIENT_HIGH_WATER) {
          state.behindFrom = start - state.waitingBytes
          state.behindSince = this.now()
          state.waiting = null
          state.waitingBytes = 0
        } else {
          state.waiting.push(frame)
          state.waitingBytes += frame.length
        }
      }
    }
  }

  // A live client takes a frame while it is within CLIENT_HIGH_WATER of its acks and, when hidden,
  // while the frame fits the budget of the current window.
  private admits(state: ClientState, bytes: number): boolean {
    if (state.sent + bytes - state.acked > CLIENT_HIGH_WATER) {
      return false
    }
    if (state.visible) {
      return true
    }
    const now = this.now()
    if (now - state.budgetFrom >= HIDDEN_BUDGET_WINDOW_MS) {
      const windows = Math.floor((now - state.budgetFrom) / HIDDEN_BUDGET_WINDOW_MS)
      state.budgetFrom += windows * HIDDEN_BUDGET_WINDOW_MS
      state.budgetBytes = 0
    }
    if (state.budgetBytes + bytes > HIDDEN_BUDGET_BYTES) {
      return false
    }
    state.budgetBytes += bytes
    return true
  }

  // The last lines of a shell as plain text. Undefined when the shell is not open.
  lastScreen(taskId: string, terminal: string, maxLines: number): string | undefined {
    return this.sessions.get(sessionKey(taskId, terminal))?.screen.lastLines(maxLines)
  }

  // A shell that a finished task kept, because a program runs in it, ends once it is idle, also
  // when no dashboard ever attaches to it.
  endWhenIdle(taskId: string, terminal: string): void {
    const session = this.sessions.get(sessionKey(taskId, terminal))
    if (session !== undefined && session.clients.size === 0) {
      this.closeWhenIdle(session)
    }
  }

  // An ended task needs no shell that nobody looks at. One that runs a program, such as a dev
  // server, is checked again later.
  private closeWhenIdle(session: Session): void {
    if (!this.isEnded(session.taskId)) {
      return
    }
    clearTimeout(session.idleTimer)
    session.idleTimer = setTimeout(() => {
      const key = sessionKey(session.taskId, session.terminal)
      if (this.sessions.get(key) !== session || session.clients.size > 0) {
        return
      }
      const program = session.pty.foreground?.()
      if (program !== undefined && !SHELLS.includes(basename(program))) {
        this.closeWhenIdle(session)
        return
      }
      this.closeOne(session.taskId, session.terminal)
    }, ENDED_IDLE_MS)
    session.idleTimer.unref()
  }

  // Resolves once all output so far is on the screen, every client that attached before got its
  // screen, and the output so far is sent.
  async settled(taskId: string, terminal = MAIN_TERMINAL): Promise<void> {
    const key = sessionKey(taskId, terminal)
    const session = this.sessions.get(key)
    while (session !== undefined && this.sessions.get(key) === session) {
      await session.screen.settled()
      const job = session.snapshot
      if (job === undefined) {
        break
      }
      await job.done
    }
    if (session !== undefined && this.sessions.get(key) === session) {
      this.flushOutput(session)
    }
  }

  // Types a command line into a shell of the task (the first by default), starting the shell
  // first when needed.
  run(taskId: string, line: string, terminal = MAIN_TERMINAL, size = DEFAULT_SIZE): void {
    this.write(this.ensure(taskId, terminal, size), `${line}\r`)
  }

  // Pastes text into a shell (the first by default) and presses Enter, so the agent there takes it
  // as its next prompt. Bracketed paste keeps line breaks from sending the text early. The Cursor
  // CLI ignores an Enter that it reads in one piece with the end of a paste, so the Enter goes
  // ENTER_AFTER_PASTE_MS later. The text loses escapes and control bytes: one could end the paste
  // early, and then the agent reads the Enter as a line break.
  paste(taskId: string, text: string, terminal = MAIN_TERMINAL): void {
    const session = this.sessions.get(sessionKey(taskId, terminal))
    if (!session) {
      throw new NoTerminalError(taskId)
    }
    this.write(session, `\x1b[200~${plainText(text)}\x1b[201~`)
    session.pending.push({ data: '\r', kind: 'submit' })
    this.flush(session)
  }

  // The program in the foreground of a shell, when the platform tells.
  foreground(taskId: string, terminal = MAIN_TERMINAL): string | undefined {
    return this.sessions.get(sessionKey(taskId, terminal))?.pty.foreground?.()
  }

  // The command lines of the programs in a shell, when the platform tells.
  async commands(taskId: string, terminal = MAIN_TERMINAL): Promise<string[] | undefined> {
    return this.sessions.get(sessionKey(taskId, terminal))?.pty.commands?.()
  }

  // Input from a client gives the shell that client's size first: the user types where they look.
  input(taskId: string, data: string, terminal = MAIN_TERMINAL, client?: TerminalClient): void {
    const session = this.sessions.get(sessionKey(taskId, terminal))
    if (!session) {
      return
    }
    const state = client && this.touch(session, client)
    if (state) {
      this.setSize(session, state.size)
    }
    this.write(session, data)
  }

  // A Vietnamese input method types "a", then erases it with DEL and types "á". Claude Code drops
  // the text that it reads in one piece with a DEL, and pieces sent at the same moment are read
  // together. So DEL and other input go to the shell apart, INPUT_GAP_MS from each other. Typing
  // without DEL, or holding Backspace, is not slowed down.
  private write(session: Session, data: string): void {
    for (const piece of data.match(/\x7f+|[^\x7f]+/g) ?? []) {
      session.pending.push({ data: piece, kind: piece.startsWith('\x7f') ? 'erase' : 'text' })
    }
    this.flush(session)
  }

  private flush(session: Session): void {
    if (session.writeTimer !== undefined) {
      return
    }
    for (let part = session.pending[0]; part !== undefined; part = session.pending[0]) {
      const { data, kind } = part
      const last = session.lastWrite
      const gap = kind === 'submit' ? ENTER_AFTER_PASTE_MS : INPUT_GAP_MS
      const wait = last && last.kind !== kind ? last.at + gap - this.now() : 0
      if (wait > 0) {
        session.writeTimer = setTimeout(() => {
          session.writeTimer = undefined
          this.flush(session)
        }, wait)
        return
      }
      session.pending.shift()
      session.pty.write(data)
      session.lastWrite = { at: this.now(), kind }
    }
  }

  // A client sends its size when it changes and when it takes the focus.
  resize(
    taskId: string,
    size: TerminalSize,
    terminal = MAIN_TERMINAL,
    client?: TerminalClient,
  ): void {
    const session = this.sessions.get(sessionKey(taskId, terminal))
    if (!session) {
      return
    }
    const state = client && this.touch(session, client)
    if (state) {
      state.size = size
    }
    this.setSize(session, size)
  }

  // Makes a client the one used last, and the one that claimed the size.
  private touch(session: Session, client: TerminalClient): ClientState | undefined {
    const state = session.clients.get(client)
    if (state) {
      session.clients.delete(client)
      session.clients.set(client, state)
      state.claimed = true
    }
    return state
  }

  // Only the daemon's screen pauses a shell; a slow client never does.
  private setPaused(session: Session, paused: boolean): void {
    if (session.paused === paused) {
      return
    }
    session.paused = paused
    if (paused) {
      session.pty.pause()
    } else {
      session.pty.resume()
    }
  }

  // The shell is told only when the size really changes. Output drawn for the old size goes out
  // first.
  private setSize(session: Session, size: TerminalSize): void {
    if (!sameSize(session.size, size)) {
      this.flushOutput(session)
      session.size = size
      session.pty.resize(size.cols, size.rows)
      session.screen.resize(size)
    }
  }

  // Ends one shell of a task. It is forgotten at once, so a new shell with the same name starts a
  // new process with an empty screen, even when the old process is slow to end.
  closeOne(taskId: string, terminal: string): void {
    const key = sessionKey(taskId, terminal)
    const session = this.sessions.get(key)
    if (!session) {
      return
    }
    this.sessions.delete(key)
    this.bus?.publish({ kind: 'shells.changed', taskId })
    clearTimeout(session.idleTimer)
    clearTimeout(session.writeTimer)
    clearTimeout(session.resyncTimer)
    this.flushOutput(session)
    this.endSnapshots(session)
    session.screen.dispose()
    for (const client of session.clients.keys()) {
      client.send({ type: 'exit', code: null })
    }
    session.clients.clear()

    // A shell that ignores SIGHUP (or a program in it that does) must not stay alive.
    const timer = setTimeout(() => session.pty.kill('SIGKILL'), KILL_GRACE_MS)
    timer.unref()
    session.stopKillTimer = () => clearTimeout(timer)
    // A paused shell blocks on its output and might never get to end.
    this.setPaused(session, false)
    session.pty.kill('SIGHUP')
  }

  // Ends every shell of a task, for example before its workspace is removed.
  close(taskId: string): void {
    for (const terminal of this.list(taskId)) {
      this.closeOne(taskId, terminal)
    }
  }

  // Ends every shell of a task and waits until they have ended. Windows does not let git delete a
  // folder that a process still has open. The wait ends a moment after a stubborn shell is killed.
  closeAndWait(taskId: string): Promise<void> {
    const sessions = [...this.sessions.values()].filter((session) => session.taskId === taskId)
    return this.waitForEnd(sessions, () => this.close(taskId))
  }

  // At shutdown: the daemon must not exit before a shell that ignores SIGHUP is killed, or it
  // lives on with no parent.
  closeAllAndWait(): Promise<void> {
    return this.waitForEnd([...this.sessions.values()], () => this.closeAll())
  }

  closeAll(): void {
    for (const session of [...this.sessions.values()]) {
      this.closeOne(session.taskId, session.terminal)
    }
  }

  private async waitForEnd(sessions: Session[], close: () => void): Promise<void> {
    const ended = Promise.all(sessions.map(exitOf))
    close()
    let timer: NodeJS.Timeout | undefined
    const limit = new Promise<void>((resolve) => {
      timer = setTimeout(resolve, KILL_GRACE_MS + EXIT_WAIT_MS)
    })
    try {
      await Promise.race([ended, limit])
    } finally {
      clearTimeout(timer)
    }
  }

  private ensure(taskId: string, terminal: string, size: TerminalSize): Session {
    return this.sessions.get(sessionKey(taskId, terminal)) ?? this.start(taskId, terminal, size)
  }

  private start(taskId: string, terminal: string, size: TerminalSize): Session {
    const cwd = this.workspaceOf(taskId)
    if (this.sessions.size >= MAX_SESSIONS) {
      throw new TerminalLimitError(MAX_SESSIONS)
    }
    const pty = this.spawn({
      ...size,
      cwd,
      // The task id lets `natamux hook` name the task even when the agent changes folders, and the
      // terminal name tells which stage of a group the agent works on.
      // PORT is the first port of the task's own block, for a dev server.
      env: {
        ...this.env,
        TERM: 'xterm-256color',
        NATAMUX_TASK_ID: taskId,
        NATAMUX_TERMINAL: terminal,
        NATAMUX_PORT: String(portRange(taskId).first),
        PORT: String(portRange(taskId).first),
      },
    })
    const key = sessionKey(taskId, terminal)
    const session: Session = {
      taskId,
      terminal,
      shellId: randomBytes(16).toString('base64url'),
      pty,
      program: pty.foreground?.(),
      size,
      screen: new ScreenState(size, this.now, (full) => this.setPaused(session, full)),
      clients: new Map(),
      offset: 0,
      batch: [],
      batchBytes: 0,
      lastFlushAt: Number.NEGATIVE_INFINITY,
      ring: [],
      ringBytes: 0,
      ringStart: 0,
      nextSnapshot: new Set(),
      paused: false,
      recent: [],
      recentBytes: 0,
      pending: [],
    }
    this.sessions.set(key, session)
    this.bus?.publish({ kind: 'shells.changed', taskId })

    pty.onData((read) => {
      if (this.sessions.get(key) !== session) {
        // A closed shell may still print while it ends. Nobody looks at it any more.
        return
      }
      const all = session.partial === undefined ? read : Buffer.concat([session.partial, read])
      const end = wholeChars(all)
      // Copied: a view would keep the whole read alive.
      session.partial = end < all.length ? Buffer.from(all.subarray(end)) : undefined
      const data = all.subarray(0, end)
      if (data.length === 0) {
        return
      }
      session.screen.write(data)
      this.queueOutput(session, data)
      try {
        this.onOutput(taskId, terminal, data)
      } catch {
        // A failing listener must not break the terminal.
      }
    })
    pty.onExit((code) => {
      session.stopKillTimer?.()
      clearTimeout(session.idleTimer)
      clearTimeout(session.writeTimer)
      clearTimeout(session.resyncTimer)
      // After closeOne, nothing is pending and no client is left, so nothing goes out twice.
      this.flushOutput(session)
      this.endSnapshots(session)
      if (this.sessions.get(key) === session) {
        this.sessions.delete(key)
        session.screen.dispose()
        this.bus?.publish({ kind: 'shells.changed', taskId })
      }
      this.setPaused(session, false)
      for (const client of session.clients.keys()) {
        client.send({ type: 'exit', code })
      }
      session.clients.clear()
    })
    return session
  }

  private dropExpiredTickets(): void {
    const now = this.now()
    for (const [ticket, entry] of this.tickets) {
      if (entry.expiresAt <= now) {
        this.tickets.delete(ticket)
      }
    }
  }
}

function sameText(a: string, b: string): boolean {
  const left = Buffer.from(a)
  const right = Buffer.from(b)
  return left.length === right.length && timingSafeEqual(left, right)
}
