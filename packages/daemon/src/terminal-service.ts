import { randomBytes, timingSafeEqual } from 'node:crypto'
import { portRange, type TerminalServerMessage, type TerminalSize } from '@helmsman/core'

// The part of a pseudo terminal that the service uses. `node-pty` gives one in main.ts; tests give
// a fake, so they do not need the native module.
export type PtyProcess = {
  onData: (listener: (data: string) => void) => void
  onExit: (listener: (code: number | null) => void) => void
  write: (data: string) => void
  resize: (cols: number, rows: number) => void
  // A signal name such as 'SIGHUP'. Without one, the default of the platform.
  kill: (signal?: string) => void
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

// What a dashboard that connects later sees first, so the screen is not empty.
export const SCROLLBACK_CHARS = 200_000
export const MAX_SESSIONS = 20
export const TICKET_TTL_MS = 30_000
// A new dashboard gets the last screen and this many lines before it, not the whole history.
export const REPLAY_SCROLLBACK_LINES = 1000
// A shell gets this long to end after SIGHUP, then it is killed.
export const KILL_GRACE_MS = 2000
// The size of a shell started before any dashboard looks at it. The first resize fixes it.
const DEFAULT_SIZE: TerminalSize = { cols: 120, rows: 32 }

// A task can have several terminals, named "1", "2" and so on. The agent runs in the first.
export const MAIN_TERMINAL = '1'

// The last `count` lines of a text, line breaks kept.
export function lastLines(text: string, count: number): string {
  // A text that ends with a line break has an empty last line, which is not counted.
  let start = text.endsWith('\n') ? text.length - 1 : text.length
  for (let seen = 0; seen < count; seen += 1) {
    if (start <= 0) {
      return text
    }
    const found = text.lastIndexOf('\n', start - 1)
    if (found === -1) {
      return text
    }
    start = found
  }
  return text.slice(start + 1)
}

function sessionKey(taskId: string, terminal: string): string {
  return `${taskId}/${terminal}`
}

type Session = {
  taskId: string
  terminal: string
  pty: PtyProcess
  scrollback: string
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
// started in it goes on; the next dashboard that connects gets the recent output first.
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
    this.env = options.env ?? process.env
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
    if (session.scrollback !== '') {
      client.send({
        type: 'output',
        data: lastLines(session.scrollback, size.rows + REPLAY_SCROLLBACK_LINES),
      })
    }
    session.clients.add(client)
    return () => {
      session.clients.delete(client)
    }
  }

  // Types a command line into the task's first shell, starting the shell first when needed.
  run(taskId: string, line: string, size: TerminalSize = DEFAULT_SIZE): void {
    this.ensure(taskId, MAIN_TERMINAL, size).pty.write(`${line}\r`)
  }

  // Pastes text into the first shell and presses Enter, so the agent there takes it as its next
  // prompt. Bracketed paste keeps line breaks from sending the text early.
  paste(taskId: string, text: string): void {
    const session = this.sessions.get(sessionKey(taskId, MAIN_TERMINAL))
    if (!session) {
      throw new NoTerminalError(taskId)
    }
    session.pty.write(`\x1b[200~${text}\x1b[201~\r`)
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
    session.scrollback = ''
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
      // The task id lets `helmsman hook` name the task even when the agent changes folders.
      // PORT is the first port of the task's own block, for a dev server.
      env: {
        ...this.env,
        TERM: 'xterm-256color',
        HELMSMAN_TASK_ID: taskId,
        HELMSMAN_PORT: String(portRange(taskId).first),
        PORT: String(portRange(taskId).first),
      },
    })
    const key = sessionKey(taskId, terminal)
    const session: Session = { taskId, terminal, pty, size, scrollback: '', clients: new Set() }
    this.sessions.set(key, session)

    pty.onData((data) => {
      if (this.sessions.get(key) !== session) {
        // A closed shell may still print while it ends. Nobody looks at it any more.
        return
      }
      session.scrollback = (session.scrollback + data).slice(-SCROLLBACK_CHARS)
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
