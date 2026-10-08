import { randomBytes, timingSafeEqual } from 'node:crypto'
import type { TerminalServerMessage, TerminalSize } from '@helmsman/core'

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

type Session = {
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

// One shell per task, in its workspace. The shell keeps running when the dashboard closes, so an
// agent started in it goes on; the next dashboard that connects gets the recent output first.
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

  isOpen(taskId: string): boolean {
    return this.sessions.has(taskId)
  }

  // Starts the shell when the task has none. Returns the function that detaches the client.
  attach(taskId: string, client: TerminalClient, size: TerminalSize): () => void {
    const session = this.ensure(taskId, size)
    if (session.scrollback !== '') {
      client.send({ type: 'output', data: session.scrollback })
    }
    session.clients.add(client)
    return () => {
      session.clients.delete(client)
    }
  }

  // Types a command line into the task's shell, starting the shell first when needed.
  run(taskId: string, line: string, size: TerminalSize = DEFAULT_SIZE): void {
    this.ensure(taskId, size).pty.write(`${line}\r`)
  }

  input(taskId: string, data: string): void {
    this.sessions.get(taskId)?.pty.write(data)
  }

  resize(taskId: string, size: TerminalSize): void {
    this.sessions.get(taskId)?.pty.resize(size.cols, size.rows)
  }

  // Ends the shell of a task, for example before its workspace is removed.
  close(taskId: string): void {
    this.sessions.get(taskId)?.pty.kill()
  }

  closeAll(): void {
    for (const session of this.sessions.values()) {
      session.pty.kill()
    }
  }

  private ensure(taskId: string, size: TerminalSize): Session {
    return this.sessions.get(taskId) ?? this.start(taskId, size)
  }

  private start(taskId: string, size: TerminalSize): Session {
    const cwd = this.workspaceOf(taskId)
    if (this.sessions.size >= MAX_SESSIONS) {
      throw new TerminalLimitError(MAX_SESSIONS)
    }
    const pty = this.spawn({
      ...size,
      cwd,
      // The task id lets `helmsman hook` name the task even when the agent changes folders.
      env: { ...this.env, TERM: 'xterm-256color', HELMSMAN_TASK_ID: taskId },
    })
    const session: Session = { pty, scrollback: '', clients: new Set() }
    this.sessions.set(taskId, session)

    pty.onData((data) => {
      session.scrollback = (session.scrollback + data).slice(-SCROLLBACK_CHARS)
      for (const client of session.clients) {
        client.send({ type: 'output', data })
      }
    })
    pty.onExit((code) => {
      if (this.sessions.get(taskId) === session) {
        this.sessions.delete(taskId)
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
