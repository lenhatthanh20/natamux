import { randomBytes, timingSafeEqual } from 'node:crypto'
import { portRange, type TerminalServerMessage, type TerminalSize } from '@helmsman/core'

// The part of a pseudo terminal that the service uses. `node-pty` gives one in main.ts; tests give
// a fake, so they do not need the native module.
export type PtyProcess = {
  onData: (listener: (data: string) => void) => void
  onExit: (listener: (code: number | null) => void) => void
  write: (data: string) => void
  resize: (cols: number, rows: number) => void
  kill: () => void
}

export type SpawnPty = (
  options: TerminalSize & { cwd: string; env: NodeJS.ProcessEnv },
) => PtyProcess

export type TerminalClient = {
  send: (message: TerminalServerMessage) => void
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
// The size of a shell started before any dashboard looks at it. The first resize fixes it.
const DEFAULT_SIZE: TerminalSize = { cols: 120, rows: 32 }

// A task can have several terminals, named "1", "2" and so on. The agent runs in the first.
export const MAIN_TERMINAL = '1'

function sessionKey(taskId: string, terminal: string): string {
  return `${taskId}/${terminal}`
}

type Session = {
  taskId: string
  terminal: string
  pty: PtyProcess
  scrollback: string
  clients: Set<TerminalClient>
}

type TerminalServiceOptions = {
  spawn: SpawnPty
  // The folder of the task's workspace. Throws when the task has none.
  workspaceOf: (taskId: string) => string
  env?: NodeJS.ProcessEnv
  now?: () => number
}

type Ticket = { taskId: string; expiresAt: number }

// Shells in the workspace of a task. A shell keeps running when the dashboard closes, so an agent
// started in it goes on; the next dashboard that connects gets the recent output first.
export class TerminalService {
  private readonly spawn: SpawnPty
  private readonly workspaceOf: (taskId: string) => string
  private readonly env: NodeJS.ProcessEnv
  private readonly now: () => number
  private readonly sessions = new Map<string, Session>()
  private readonly tickets = new Map<string, Ticket>()

  constructor(options: TerminalServiceOptions) {
    this.spawn = options.spawn
    this.workspaceOf = options.workspaceOf
    this.env = options.env ?? process.env
    this.now = options.now ?? Date.now
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
    const session = this.ensure(taskId, terminal, size)
    if (session.scrollback !== '') {
      client.send({ type: 'output', data: session.scrollback })
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

  input(taskId: string, data: string, terminal = MAIN_TERMINAL): void {
    this.sessions.get(sessionKey(taskId, terminal))?.pty.write(data)
  }

  resize(taskId: string, size: TerminalSize, terminal = MAIN_TERMINAL): void {
    this.sessions.get(sessionKey(taskId, terminal))?.pty.resize(size.cols, size.rows)
  }

  // Ends one shell of a task.
  closeOne(taskId: string, terminal: string): void {
    this.sessions.get(sessionKey(taskId, terminal))?.pty.kill()
  }

  // Ends every shell of a task, for example before its workspace is removed.
  close(taskId: string): void {
    for (const terminal of this.list(taskId)) {
      this.closeOne(taskId, terminal)
    }
  }

  closeAll(): void {
    for (const session of this.sessions.values()) {
      session.pty.kill()
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
    const session: Session = { taskId, terminal, pty, scrollback: '', clients: new Set() }
    this.sessions.set(key, session)

    pty.onData((data) => {
      session.scrollback = (session.scrollback + data).slice(-SCROLLBACK_CHARS)
      for (const client of session.clients) {
        client.send({ type: 'output', data })
      }
    })
    pty.onExit((code) => {
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
