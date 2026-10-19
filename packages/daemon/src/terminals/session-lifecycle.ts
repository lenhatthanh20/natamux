import { randomBytes } from 'node:crypto'
import { portRange, type TerminalSize } from '@natamux/core'
import type { EventBus } from '../shared/event-bus.js'
import { ScreenState, wholeChars } from './screen-state.js'
import { setPaused } from './session-input.js'
import { flushOutput, queueOutput, type SessionContext } from './session-output.js'
import { endSnapshots } from './session-snapshot.js'
import { EXIT_WAIT_MS, KILL_GRACE_MS } from './terminal-limits.js'
import type { Session, SpawnPty } from './terminal-types.js'

export function sessionKey(taskId: string, terminal: string): string {
  return `${taskId}/${terminal}`
}

function exitOf(session: Session): Promise<void> {
  return new Promise((resolve) => session.pty.onExit(() => resolve()))
}

export function spawnSession(
  context: SessionContext,
  spawn: SpawnPty,
  env: NodeJS.ProcessEnv,
  cwd: string,
  taskId: string,
  terminal: string,
  size: TerminalSize,
): Session {
  const pty = spawn({
    ...size,
    cwd,
    // The task id lets `natamux hook` name the task even when the agent changes folders, and the
    // terminal name tells which stage of a group the agent works on.
    // PORT is the first port of the task's own block, for a dev server.
    env: {
      ...env,
      TERM: 'xterm-256color',
      NATAMUX_TASK_ID: taskId,
      NATAMUX_TERMINAL: terminal,
      NATAMUX_PORT: String(portRange(taskId).first),
      PORT: String(portRange(taskId).first),
    },
  })
  const session: Session = {
    taskId,
    terminal,
    shellId: randomBytes(16).toString('base64url'),
    pty,
    program: pty.foreground?.(),
    size,
    screen: new ScreenState(size, context.now, (full) => setPaused(session, full)),
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
  return session
}

export function wireOutput(
  context: SessionContext,
  sessions: Map<string, Session>,
  session: Session,
  onOutput: (taskId: string, terminal: string, data: Buffer) => void,
): void {
  const key = sessionKey(session.taskId, session.terminal)
  session.pty.onData((read) => {
    if (sessions.get(key) !== session) {
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
    queueOutput(context, session, data)
    try {
      onOutput(session.taskId, session.terminal, data)
    } catch {
      // A failing listener must not break the terminal.
    }
  })
}

export function wireExit(
  context: SessionContext,
  sessions: Map<string, Session>,
  session: Session,
  bus: EventBus | undefined,
): void {
  const key = sessionKey(session.taskId, session.terminal)
  session.pty.onExit((code) => {
    // A program that ignored SIGHUP can outlive its shell. It still gets the SIGKILL.
    if (!session.pty.leftover?.()) {
      session.stopKillTimer?.()
    }
    clearTimeout(session.idleTimer)
    clearTimeout(session.writeTimer)
    clearTimeout(session.resyncTimer)
    // After closeOne, nothing is pending and no client is left, so nothing goes out twice.
    flushOutput(context, session)
    endSnapshots(session)
    if (sessions.get(key) === session) {
      sessions.delete(key)
      session.screen.dispose()
      bus?.publish({ kind: 'shells.changed', taskId: session.taskId })
    }
    setPaused(session, false)
    for (const client of session.clients.keys()) {
      client.send({ type: 'exit', code })
    }
    session.clients.clear()
  })
}

// Ends a shell that the service already forgot.
export function closeSession(context: SessionContext, session: Session): void {
  clearTimeout(session.idleTimer)
  clearTimeout(session.writeTimer)
  clearTimeout(session.resyncTimer)
  flushOutput(context, session)
  endSnapshots(session)
  session.screen.dispose()
  for (const client of session.clients.keys()) {
    client.send({ type: 'exit', code: null })
  }
  session.clients.clear()

  // A shell that ignores SIGHUP (or a program in it that does) must not stay alive.
  session.killDone = new Promise((resolve) => {
    const timer = setTimeout(() => {
      session.pty.kill('SIGKILL')
      resolve()
    }, KILL_GRACE_MS)
    timer.unref()
    session.stopKillTimer = () => {
      clearTimeout(timer)
      resolve()
    }
  })
  // A paused shell blocks on its output and might never get to end.
  setPaused(session, false)
  session.pty.kill('SIGHUP')
}

export async function waitForEnd(sessions: Session[], close: () => void): Promise<void> {
  const exited = Promise.all(sessions.map(exitOf))
  close()
  const ended = Promise.all([exited, ...sessions.map((session) => session.killDone)])
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
