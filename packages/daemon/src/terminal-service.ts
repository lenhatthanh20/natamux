import { randomBytes, timingSafeEqual } from 'node:crypto'
import { basename } from 'node:path'
import {
  portRange,
  scrollbackLines,
  type TerminalServerMessage,
  type TerminalSize,
} from '@natamux/core'
import { SerializeAddon } from '@xterm/addon-serialize'
import { Unicode11Addon } from '@xterm/addon-unicode11'
// The package is CommonJS without named exports for Node.js.
import xtermHeadless from '@xterm/headless'
import { withLoopbackNoProxy } from './no-proxy.js'

// The part of a pseudo terminal that the service uses. `node-pty` gives one in main.ts; tests give
// a fake, so they do not need the native module.
export type PtyProcess = {
  onData: (listener: (data: string) => void) => void
  onExit: (listener: (code: number | null) => void) => void
  write: (data: string) => void
  resize: (cols: number, rows: number) => void
  // A signal name such as 'SIGHUP'. Without one, the default of the platform.
  kill: (signal?: string) => void
  // The name of the program in the foreground, such as "bash" or "node".
  foreground?: () => string | undefined
}

export type SpawnPty = (
  options: TerminalSize & { cwd: string; env: NodeJS.ProcessEnv },
) => PtyProcess

export type TerminalClient = {
  send: (message: TerminalServerMessage) => void
}

export class NoTerminalError extends Error {
  constructor(taskId: string) {
    super(`${taskId} has no open terminal. Start the agent first`)
    this.name = 'NoTerminalError'
  }
}

export class TerminalLimitError extends Error {
  constructor(limit: number) {
    super(`At most ${limit} terminals can be open. Close one first.`)
    this.name = 'TerminalLimitError'
  }
}

export const MAX_SESSIONS = 20
export const TICKET_TTL_MS = 30_000
// The history goes to a new dashboard in messages of this size, so no frame is megabytes long.
export const REPLAY_CHUNK_CHARS = 64 * 1024
// A shell gets this long to end after SIGHUP, then it is killed.
export const KILL_GRACE_MS = 2000
// How long a killed shell may take to end before closeAndWait stops waiting for it.
const EXIT_WAIT_MS = 1000
// Time between a DEL and other input that a program must read apart, see TerminalService.write.
export const INPUT_GAP_MS = 25
// Time between a paste and its Enter, see TerminalService.paste.
export const ENTER_AFTER_PASTE_MS = 100
// The size of a shell started before any dashboard looks at it. The first resize fixes it.
const DEFAULT_SIZE: TerminalSize = { cols: 120, rows: 32 }

// A shell of a done or cancelled task that nobody looks at ends after this long, unless a program
// runs in it. A dashboard that comes back sooner finds it as it was.
export const ENDED_IDLE_MS = 60_000

// A task can have several terminals, named "1", "2" and so on. The agent runs in the first.
export const MAIN_TERMINAL = '1'

// The names of shells. Another program in the foreground means something runs in the terminal.
export const SHELLS = [
  'bash',
  'zsh',
  'sh',
  'dash',
  'fish',
  'ksh',
  'nu',
  'pwsh',
  'powershell.exe',
  'cmd.exe',
]

function isHighSurrogate(code: number): boolean {
  return code >= 0xd800 && code <= 0xdbff
}

// The screen of a shell, kept the way a terminal shows it. Programs such as agents place text
// with cursor moves for the size the shell had at that moment, so the raw output shows garbage
// on a screen of another size. A dashboard that connects gets this screen instead, drawn again
// for its size, with the lines above it.
export class ScreenState {
  private readonly terminal: InstanceType<typeof xtermHeadless.Terminal>
  private readonly serializer = new SerializeAddon()
  // Output parses later; what comes after the shell is gone must not touch the screen.
  private disposed = false

  constructor(size: TerminalSize) {
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

  write(data: string): void {
    if (!this.disposed) {
      this.terminal.write(data)
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

  // Calls back with the screen, and the lines above it, once all output so far is on it.
  snapshot(done: (text: string) => void): void {
    this.whenParsed(() => done(this.serializer.serialize()))
  }

  // Resolves once all output so far is on the screen.
  settled(): Promise<void> {
    return new Promise((resolve) => (this.disposed ? resolve() : this.whenParsed(resolve)))
  }

  dispose(): void {
    this.disposed = true
    this.terminal.dispose()
  }

  private whenParsed(run: () => void): void {
    if (!this.disposed) {
      this.terminal.write('', () => {
        if (!this.disposed) {
          run()
        }
      })
    }
  }
}

// `text` in pieces of at most `size` chars, none of them ending inside a surrogate pair.
export function replayChunks(text: string, size = REPLAY_CHUNK_CHARS): string[] {
  const chunks: string[] = []
  let start = 0
  while (start < text.length) {
    let end = Math.min(start + size, text.length)
    if (end < text.length && end - 1 > start && isHighSurrogate(text.charCodeAt(end - 1))) {
      end -= 1
    }
    chunks.push(text.slice(start, end))
    start = end
  }
  return chunks
}

function sessionKey(taskId: string, terminal: string): string {
  return `${taskId}/${terminal}`
}

type ClientState = {
  // The size of the client's screen.
  size: TerminalSize
  // Output that came while the client waits for its first screen, or null once it has it.
  waiting: string[] | null
}

type Session = {
  taskId: string
  terminal: string
  pty: PtyProcess
  screen: ScreenState
  // The client used last is the last one in the map.
  clients: Map<TerminalClient, ClientState>
  size: TerminalSize
  // Set when the shell is being closed: the pending SIGKILL is not needed once it has ended.
  stopKillTimer?: () => void
  // Set while the shell of an ended task waits, with no client, to be closed.
  idleTimer?: NodeJS.Timeout
  // Input that waits for its turn, see write().
  pending: Input[]
  lastWrite?: { at: number; kind: Input['kind'] }
  writeTimer?: NodeJS.Timeout
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
  onOutput?: (taskId: string, terminal: string, data: string) => void
  // True when the task is done or cancelled, so its shells are only there to look around.
  isEnded?: (taskId: string) => boolean
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
  private readonly onOutput: (taskId: string, terminal: string, data: string) => void
  private readonly isEnded: (taskId: string) => boolean
  private readonly sessions = new Map<string, Session>()
  private readonly tickets = new Map<string, Ticket>()

  constructor(options: TerminalServiceOptions) {
    this.spawn = options.spawn
    this.workspaceOf = options.workspaceOf
    this.env = withLoopbackNoProxy(options.env ?? process.env)
    this.now = options.now ?? Date.now
    this.onOutput = options.onOutput ?? (() => undefined)
    this.isEnded = options.isEnded ?? (() => false)
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

  // Starts the shell when there is none. Returns the function that detaches the client.
  attach(
    taskId: string,
    client: TerminalClient,
    size: TerminalSize,
    terminal = MAIN_TERMINAL,
  ): () => void {
    const existed = this.sessions.has(sessionKey(taskId, terminal))
    const session = this.ensure(taskId, terminal, size)
    const state: ClientState = { size, waiting: [] }
    session.clients.set(client, state)
    clearTimeout(session.idleTimer)
    if (existed) {
      // The screen is drawn for this size, so the shell must know it before the replay.
      this.setSize(session, size)
    }
    session.screen.snapshot((text) => {
      if (session.clients.get(client) !== state) {
        return
      }
      for (const data of replayChunks(text)) {
        client.send({ type: 'output', data })
      }
      for (const data of state.waiting ?? []) {
        client.send({ type: 'output', data })
      }
      state.waiting = null
    })
    return () => {
      if (session.clients.get(client) !== state) {
        return
      }
      session.clients.delete(client)
      // The client used before it decides the size again.
      const last = [...session.clients.values()].at(-1)
      if (last) {
        this.setSize(session, last.size)
      } else {
        this.closeWhenIdle(session)
      }
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

  // Resolves once all output so far is on the screen, and every client that attached before got
  // its screen.
  async settled(taskId: string, terminal = MAIN_TERMINAL): Promise<void> {
    await this.sessions.get(sessionKey(taskId, terminal))?.screen.settled()
  }

  // Types a command line into a shell of the task (the first by default), starting the shell
  // first when needed.
  run(taskId: string, line: string, terminal = MAIN_TERMINAL, size = DEFAULT_SIZE): void {
    this.write(this.ensure(taskId, terminal, size), `${line}\r`)
  }

  // Pastes text into a shell (the first by default) and presses Enter, so the agent there takes it
  // as its next prompt. Bracketed paste keeps line breaks from sending the text early. The Cursor
  // CLI ignores an Enter that it reads in one piece with the end of a paste, so the Enter goes
  // ENTER_AFTER_PASTE_MS later.
  paste(taskId: string, text: string, terminal = MAIN_TERMINAL): void {
    const session = this.sessions.get(sessionKey(taskId, terminal))
    if (!session) {
      throw new NoTerminalError(taskId)
    }
    this.write(session, `\x1b[200~${text}\x1b[201~`)
    session.pending.push({ data: '\r', kind: 'submit' })
    this.flush(session)
  }

  // The program in the foreground of a shell, when the platform tells.
  foreground(taskId: string, terminal = MAIN_TERMINAL): string | undefined {
    return this.sessions.get(sessionKey(taskId, terminal))?.pty.foreground?.()
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

  // Makes a client the one used last.
  private touch(session: Session, client: TerminalClient): ClientState | undefined {
    const state = session.clients.get(client)
    if (state) {
      session.clients.delete(client)
      session.clients.set(client, state)
    }
    return state
  }

  // The shell is told only when the size really changes.
  private setSize(session: Session, size: TerminalSize): void {
    if (session.size.cols !== size.cols || session.size.rows !== size.rows) {
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
    clearTimeout(session.idleTimer)
    clearTimeout(session.writeTimer)
    session.screen.dispose()
    for (const client of session.clients.keys()) {
      client.send({ type: 'exit', code: null })
    }
    session.clients.clear()

    // A shell that ignores SIGHUP (or a program in it that does) must not stay alive.
    const timer = setTimeout(() => session.pty.kill('SIGKILL'), KILL_GRACE_MS)
    timer.unref()
    session.stopKillTimer = () => clearTimeout(timer)
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
  async closeAndWait(taskId: string): Promise<void> {
    const sessions = [...this.sessions.values()].filter((session) => session.taskId === taskId)
    const ended = Promise.all(sessions.map(exitOf))
    this.close(taskId)
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

  closeAll(): void {
    for (const session of [...this.sessions.values()]) {
      this.closeOne(session.taskId, session.terminal)
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
      pty,
      size,
      screen: new ScreenState(size),
      clients: new Map(),
      pending: [],
    }
    this.sessions.set(key, session)

    pty.onData((data) => {
      if (this.sessions.get(key) !== session) {
        // A closed shell may still print while it ends. Nobody looks at it any more.
        return
      }
      session.screen.write(data)
      for (const [client, state] of session.clients) {
        if (state.waiting) {
          state.waiting.push(data)
        } else {
          client.send({ type: 'output', data })
        }
      }
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
      if (this.sessions.get(key) === session) {
        this.sessions.delete(key)
        session.screen.dispose()
      }
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
