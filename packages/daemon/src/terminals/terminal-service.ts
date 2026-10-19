import { randomBytes, timingSafeEqual } from 'node:crypto'
import { basename } from 'node:path'
import { plainText, type TerminalSize } from '@natamux/core'
import type { EventBus } from '../shared/event-bus.js'
import { withLoopbackNoProxy } from './no-proxy.js'
import { flush, setSize, touch, write } from './session-input.js'
import {
  closeSession,
  sessionKey,
  spawnSession,
  waitForEnd,
  wireExit,
  wireOutput,
} from './session-lifecycle.js'
import { flushOutput, ringSince, type SessionContext } from './session-output.js'
import { catchUpBehind, requestSnapshot, resume } from './session-snapshot.js'
import {
  ENDED_IDLE_MS,
  MAIN_TERMINAL,
  MAX_SESSIONS,
  SHELLS,
  TICKET_TTL_MS,
  WORKSPACE_QUIET_MS,
} from './terminal-limits.js'
import {
  type ClientState,
  DEFAULT_SIZE,
  NoTerminalError,
  type Session,
  ShellGoneError,
  type SpawnPty,
  type TerminalClient,
  TerminalLimitError,
  type TerminalServiceOptions,
  type Ticket,
} from './terminal-types.js'

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
  private readonly context: SessionContext
  // By task: whether output came since the quiet timer was set.
  private readonly outputSeen = new Map<string, { seen: boolean }>()

  constructor(options: TerminalServiceOptions) {
    this.spawn = options.spawn
    this.workspaceOf = options.workspaceOf
    this.env = withLoopbackNoProxy(options.env ?? process.env)
    this.now = options.now ?? Date.now
    this.context = { now: this.now, catchUp: (session) => catchUpBehind(this.context, session) }
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
        setSize(this.context, session, size)
      }
    } else {
      // First, as the one used least: another client leaving never gives the shell its size.
      session.clients = new Map<TerminalClient, ClientState>([[client, state], ...session.clients])
    }
    clearTimeout(session.idleTimer)
    // No output reaches it before: it is in the snapshot phase with nothing waiting.
    client.send({ type: 'hello', shellId: session.shellId })
    if (resumed === undefined || !resume(this.context, session, client, state, options.offset)) {
      requestSnapshot(this.context, session, client, state)
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
        setSize(this.context, session, last.size)
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
      touch(session, client)
      state.size = { cols: message.cols, rows: message.rows }
      setSize(this.context, session, state.size)
    }
    if (message.visible && state.phase === 'behind') {
      catchUpBehind(this.context, session)
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
      catchUpBehind(this.context, session)
    }
  }

  // The output sent since `offset`, while it is still kept. Undefined when it is not (too old,
  // in the future, or inside a frame).
  outputSince(taskId: string, offset: number, terminal = MAIN_TERMINAL): Buffer[] | undefined {
    const session = this.sessions.get(sessionKey(taskId, terminal))
    return session ? ringSince(session, offset) : undefined
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
      flushOutput(this.context, session)
    }
  }

  // Types a command line into a shell of the task (the first by default), starting the shell
  // first when needed.
  run(taskId: string, line: string, terminal = MAIN_TERMINAL, size = DEFAULT_SIZE): void {
    write(this.context, this.ensure(taskId, terminal, size), `${line}\r`)
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
    write(this.context, session, `\x1b[200~${plainText(text)}\x1b[201~`)
    session.pending.push({ data: '\r', kind: 'submit' })
    flush(this.context, session)
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
    const state = client && touch(session, client)
    if (state) {
      setSize(this.context, session, state.size)
    }
    write(this.context, session, data)
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
    const state = client && touch(session, client)
    if (state) {
      state.size = size
    }
    setSize(this.context, session, size)
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
    closeSession(this.context, session)
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
    return waitForEnd(sessions, () => this.close(taskId))
  }

  // At shutdown: the daemon must not exit before a shell that ignores SIGHUP is killed, or it
  // lives on with no parent.
  closeAllAndWait(): Promise<void> {
    return waitForEnd([...this.sessions.values()], () => this.closeAll())
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
    const session = spawnSession(this.context, this.spawn, this.env, cwd, taskId, terminal, size)
    this.sessions.set(sessionKey(taskId, terminal), session)
    this.bus?.publish({ kind: 'shells.changed', taskId })

    wireOutput(this.context, this.sessions, session, this.onOutput)
    wireExit(this.context, this.sessions, session, this.bus)
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
