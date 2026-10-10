import { randomBytes, timingSafeEqual } from 'node:crypto'
import {
  portRange,
  TERMINAL_HISTORY_CHARS,
  type TerminalServerMessage,
  type TerminalSize,
} from '@natamux/core'
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
// How far past the cut the history looks for a line break or an escape to start at.
const CUT_SEARCH_CHARS = 10_000
// A shell gets this long to end after SIGHUP, then it is killed.
export const KILL_GRACE_MS = 2000
// The size of a shell started before any dashboard looks at it. The first resize fixes it.
const DEFAULT_SIZE: TerminalSize = { cols: 120, rows: 32 }

// A task can have several terminals, named "1", "2" and so on. The agent runs in the first.
export const MAIN_TERMINAL = '1'

function isHighSurrogate(code: number): boolean {
  return code >= 0xd800 && code <= 0xdbff
}

function isLowSurrogate(code: number): boolean {
  return code >= 0xdc00 && code <= 0xdfff
}

// Where to cut the start of `text` to drop at least `from` chars. A replay that starts with half a
// line or half an escape sequence shows garbage, so the cut moves to the next line, or else to the
// next escape, or else off the second half of a surrogate pair.
function cutPoint(text: string, from: number): number {
  if (from === 0 || text[from - 1] === '\n') {
    return from
  }
  const line = text.indexOf('\n', from)
  if (line !== -1 && line - from < CUT_SEARCH_CHARS) {
    return line + 1
  }
  const sequence = text.indexOf('\u001b', from)
  if (sequence !== -1 && sequence - from < CUT_SEARCH_CHARS) {
    return sequence
  }
  return isLowSurrogate(text.charCodeAt(from)) ? from + 1 : from
}

// The output of a shell: the newest output, never more than `limit` chars.
export class OutputHistory {
  private chunks: string[] = []
  private chars = 0

  constructor(private readonly limit = TERMINAL_HISTORY_CHARS) {}

  get size(): number {
    return this.chars
  }

  append(data: string): void {
    this.chunks.push(data)
    this.chars += data.length
    if (this.chars > this.limit) {
      // A trim copies the whole history, so it frees an eighth of the limit, not just one chunk.
      this.keepLast(this.limit - Math.floor(this.limit / 8))
    }
  }

  text(): string {
    if (this.chunks.length > 1) {
      this.chunks = [this.chunks.join('')]
    }
    return this.chunks[0] ?? ''
  }

  clear(): void {
    this.chunks = []
    this.chars = 0
  }

  private keepLast(count: number): void {
    const text = this.chunks.join('')
    const kept = text.slice(cutPoint(text, text.length - count))
    this.chunks = kept === '' ? [] : [kept]
    this.chars = kept.length
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

type Session = {
  taskId: string
  terminal: string
  pty: PtyProcess
  history: OutputHistory
  clients: Set<TerminalClient>
  size: TerminalSize
  // Set when the shell is being closed: the pending SIGKILL is not needed once it has ended.
  stopKillTimer?: () => void
}

type TerminalServiceOptions = {
  spawn: SpawnPty
  // The folder of the task's workspace. Throws when the task has none.
  workspaceOf: (taskId: string) => string
  env?: NodeJS.ProcessEnv
  now?: () => number
  // Called with every piece of output, for signs of life of an agent without hooks.
  onOutput?: (taskId: string, terminal: string, data: string) => void
}

type Ticket = { taskId: string; expiresAt: number }

// Shells in the workspace of a task. A shell keeps running when the dashboard closes, so an agent
// started in it goes on; the next dashboard that connects gets the output so far first.
export class TerminalService {
  private readonly spawn: SpawnPty
  private readonly workspaceOf: (taskId: string) => string
  private readonly env: NodeJS.ProcessEnv
  private readonly now: () => number
  private readonly onOutput: (taskId: string, terminal: string, data: string) => void
  private readonly sessions = new Map<string, Session>()
  private readonly tickets = new Map<string, Ticket>()

  constructor(options: TerminalServiceOptions) {
    this.spawn = options.spawn
    this.workspaceOf = options.workspaceOf
    this.env = withLoopbackNoProxy(options.env ?? process.env)
    this.now = options.now ?? Date.now
    this.onOutput = options.onOutput ?? (() => undefined)
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
    if (existed) {
      // The screen is drawn for this size, so the shell must know it before the replay.
      this.setSize(session, size)
    }
    for (const data of replayChunks(session.history.text())) {
      client.send({ type: 'output', data })
    }
    session.clients.add(client)
    return () => {
      session.clients.delete(client)
    }
  }

  // Types a command line into a shell of the task (the first by default), starting the shell
  // first when needed.
  run(taskId: string, line: string, terminal = MAIN_TERMINAL, size = DEFAULT_SIZE): void {
    this.ensure(taskId, terminal, size).pty.write(`${line}\r`)
  }

  // Pastes text into a shell (the first by default) and presses Enter, so the agent there takes it
  // as its next prompt. Bracketed paste keeps line breaks from sending the text early.
  paste(taskId: string, text: string, terminal = MAIN_TERMINAL): void {
    const session = this.sessions.get(sessionKey(taskId, terminal))
    if (!session) {
      throw new NoTerminalError(taskId)
    }
    session.pty.write(`\x1b[200~${text}\x1b[201~\r`)
  }

  // The program in the foreground of a shell, when the platform tells.
  foreground(taskId: string, terminal = MAIN_TERMINAL): string | undefined {
    return this.sessions.get(sessionKey(taskId, terminal))?.pty.foreground?.()
  }

  input(taskId: string, data: string, terminal = MAIN_TERMINAL): void {
    this.sessions.get(sessionKey(taskId, terminal))?.pty.write(data)
  }

  resize(taskId: string, size: TerminalSize, terminal = MAIN_TERMINAL): void {
    const session = this.sessions.get(sessionKey(taskId, terminal))
    if (session) {
      this.setSize(session, size)
    }
  }

  // The shell is told only when the size really changes.
  private setSize(session: Session, size: TerminalSize): void {
    if (session.size.cols !== size.cols || session.size.rows !== size.rows) {
      session.size = size
      session.pty.resize(size.cols, size.rows)
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
    session.history.clear()
    for (const client of session.clients) {
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
      history: new OutputHistory(),
      clients: new Set(),
    }
    this.sessions.set(key, session)

    pty.onData((data) => {
      if (this.sessions.get(key) !== session) {
        // A closed shell may still print while it ends. Nobody looks at it any more.
        return
      }
      session.history.append(data)
      for (const client of session.clients) {
        client.send({ type: 'output', data })
      }
      try {
        this.onOutput(taskId, terminal, data)
      } catch {
        // A failing listener must not break the terminal.
      }
    })
    pty.onExit((code) => {
      session.stopKillTimer?.()
      if (this.sessions.get(key) === session) {
        this.sessions.delete(key)
      }
      for (const client of session.clients) {
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
