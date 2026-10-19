import { basename } from 'node:path'
import { isLaunchable } from '../agents/agent-launch.js'
import type { EventBus } from '../shared/event-bus.js'
import {
  deleteOpenShells,
  type OpenShell,
  readOpenShells,
  saveOpenShells,
} from './open-shells-file.js'
import { SHELLS } from './terminal-limits.js'
import type { TerminalService } from './terminal-service.js'

// A restored or restarted agent shows a spinner until it is in the foreground, at most this long.
export const RESTORE_SPINNER_MS = 15_000
export const RESTORE_POLL_MS = 500
// An agent that quit, or a shell that closed, is written down only after this long. When the app
// is killed or the machine shuts down, they can end a moment before the daemon, and must come back.
export const EXIT_CONFIRM_MS = 3000

type Entry = { agent: string | null; session: string | null }
type Starting = { taskId: string; terminal: string; agent: string; until: number }

const PLAIN: Entry = { agent: null, session: null }

const keyOf = (taskId: string, terminal: string) => `${taskId}/${terminal}`

function clearTimer(timers: Map<string, NodeJS.Timeout>, key: string): void {
  clearTimeout(timers.get(key))
  timers.delete(key)
}

type OpenShellsOptions = {
  dataDir: string
  terminals: Pick<TerminalService, 'list' | 'isOpen' | 'foreground'>
  bus: EventBus
  // True for a task that is done, cancelled or gone: it keeps no shells.
  isEnded: (taskId: string) => boolean
  // Whether an agent runs in a shell (StageRunner.agentRuns). Without it, no quit is noticed.
  agentRuns?: (taskId: string, terminal: string) => boolean
  // The agent whose process runs in a shell (StageRunner.agentIn), for one typed by hand.
  agentIn?: (taskId: string, terminal: string) => Promise<string | null>
  now?: () => number
  save?: (dataDir: string, taskId: string, shells: OpenShell[]) => void
}

// The open shells of each task, kept in a file beside the task as they change, so a crash or a
// kill without SIGTERM (Windows) loses nothing. After a restart, the shells that no stage
// restarts wait until a dashboard opens the task (see StageRunner.restoreTask).
export class OpenShells {
  private readonly dataDir: string
  private readonly terminals: OpenShellsOptions['terminals']
  private readonly bus: EventBus
  private readonly isEnded: (taskId: string) => boolean
  private readonly agentRuns: OpenShellsOptions['agentRuns']
  private readonly agentIn: OpenShellsOptions['agentIn']
  private readonly now: () => number
  private readonly save: NonNullable<OpenShellsOptions['save']>
  private readonly entries = new Map<string, Map<string, Entry>>()
  // Shells from the file that have not started again yet, by task.
  private readonly waitingShells = new Map<string, Set<string>>()
  // Shells from the file whose stage starts its agent again (resumeAgents), by task.
  private readonly held = new Map<string, Set<string>>()
  // "<task>/<terminal>" of the shells where an agent was launched or seen in this run. Only their
  // quit is written down: a shell whose agent could not come back keeps it for the next try.
  private readonly active = new Set<string>()
  private readonly exitTimers = new Map<string, NodeJS.Timeout>()
  // Shells that closed, until the close is written down.
  private readonly closing = new Map<string, NodeJS.Timeout>()
  // The program last asked about in each shell with no agent, so `ps` runs once per program.
  private readonly probed = new Map<string, string>()
  // Agents just started again, by "<task>/<terminal>", until they are in the foreground.
  private readonly starting = new Map<string, Starting>()
  private poll: NodeJS.Timeout | undefined
  private stopped = false
  private unsubscribe: (() => void) | undefined

  constructor(options: OpenShellsOptions) {
    this.dataDir = options.dataDir
    this.terminals = options.terminals
    this.bus = options.bus
    this.isEnded = options.isEnded
    this.agentRuns = options.agentRuns
    this.agentIn = options.agentIn
    this.now = options.now ?? Date.now
    this.save = options.save ?? saveOpenShells
  }

  start(): void {
    this.unsubscribe = this.bus.subscribe((message) => {
      if (message.kind === 'shells.changed') {
        this.sync(message.taskId)
      } else if (message.kind === 'task.deleted') {
        this.forget(message.taskId)
      } else if (message.kind === 'task.updated' && message.event.type === 'workspace.removed') {
        // Its shells cannot start without the folder.
        this.forget(message.task.id)
      }
    })
  }

  // Reads the task's file after a restart. The shells in `skip` (those of stages whose agent
  // starts again at once) are held until releaseHeld; the rest wait for a dashboard.
  load(taskId: string, skip: readonly string[]): void {
    const shells = readOpenShells(this.dataDir, taskId)
    if (shells.length === 0) {
      return
    }
    // An agent with no session comes back on the newest session of the folder; a second of its
    // kind would open that one too, so it comes back plain. Held shells come back by their stage.
    const newest = new Set<string>()
    const entries = new Map<string, Entry>()
    for (const { name, agent, session } of shells) {
      const byNewest = agent !== null && session === null && !skip.includes(name)
      const second = byNewest && newest.has(agent)
      if (byNewest) {
        newest.add(agent)
      }
      entries.set(name, second ? PLAIN : { agent, session })
    }
    this.entries.set(taskId, entries)
    const closed = shells
      .map((shell) => shell.name)
      .filter((name) => !this.terminals.isOpen(taskId, name))
    const waiting = closed.filter((name) => !skip.includes(name))
    const held = closed.filter((name) => skip.includes(name))
    if (waiting.length > 0) {
      this.waitingShells.set(taskId, new Set(waiting))
    }
    if (held.length > 0) {
      this.held.set(taskId, new Set(held))
    }
  }

  // After resumeAgents: a held shell whose agent did not start (the task waits for other tasks, a
  // launch failed) waits for a dashboard like the others.
  releaseHeld(): void {
    for (const [taskId, names] of this.held) {
      const left = [...names].filter((name) => !this.terminals.isOpen(taskId, name))
      if (left.length > 0) {
        const waiting = this.waitingShells.get(taskId) ?? new Set<string>()
        for (const name of left) {
          waiting.add(name)
        }
        this.waitingShells.set(taskId, waiting)
        this.bus.publish({ kind: 'shells.changed', taskId })
      }
    }
    this.held.clear()
  }

  // Whether the shell had an agent when the daemon last ran: one the person quit stays quit.
  wasRunning(taskId: string, terminal: string): boolean {
    return (this.entries.get(taskId)?.get(terminal)?.agent ?? null) !== null
  }

  // The last session that `agent` reported in the shell. Hooks that change nothing are not logged
  // as events, so this can be newer than the last one in the task's events.
  sessionOf(taskId: string, terminal: string, agent: string): string | undefined {
    const entry = this.entries.get(taskId)?.get(terminal)
    return entry?.agent === agent ? (entry.session ?? undefined) : undefined
  }

  // A restore whose agent could not start: the shell waits again, for the next open or start.
  putBack(taskId: string, terminal: string): void {
    if (
      this.stopped ||
      this.terminals.isOpen(taskId, terminal) ||
      this.entries.get(taskId)?.has(terminal) !== true
    ) {
      return
    }
    const waiting = this.waitingShells.get(taskId) ?? new Set<string>()
    waiting.add(terminal)
    this.waitingShells.set(taskId, waiting)
    this.bus.publish({ kind: 'shells.changed', taskId })
  }

  waiting(taskId: string): string[] {
    return [...(this.waitingShells.get(taskId) ?? [])]
  }

  // The shells that are coming back, with the agent they come back with (null: a plain shell).
  restoring(taskId: string): Record<string, string | null> {
    const entries = this.entries.get(taskId)
    const restoring: Record<string, string | null> = {}
    for (const name of this.waiting(taskId)) {
      const entry = entries?.get(name)
      restoring[name] = entry !== undefined && resumable(entry) ? entry.agent : null
    }
    for (const { taskId: id, terminal, agent } of this.starting.values()) {
      if (id === taskId) {
        restoring[terminal] = agent
      }
    }
    return restoring
  }

  // Takes a waiting shell out of the wait, once: two dashboards that attach at once start it
  // once. Undefined when it does not wait, or a shell of that name runs already.
  take(taskId: string, terminal: string): OpenShell | undefined {
    const waiting = this.waitingShells.get(taskId)
    if (this.stopped || waiting?.delete(terminal) !== true) {
      return undefined
    }
    if (this.terminals.isOpen(taskId, terminal)) {
      return undefined
    }
    const entry = this.entries.get(taskId)?.get(terminal)
    return entry === undefined
      ? undefined
      : { name: terminal, agent: resumable(entry) ? entry.agent : null, session: entry.session }
  }

  // An agent was started in a shell, on `session` when it went on with one.
  noteLaunch(taskId: string, terminal: string, agent: string, session?: string): void {
    this.note(taskId, terminal, { agent, session: session ?? null })
  }

  // A hook named the session of the agent in a shell.
  noteSession(taskId: string, terminal: string, agent: string, session: string): void {
    this.note(taskId, terminal, { agent, session })
  }

  // The agent of a shell is starting again: its pane shows a spinner until it runs.
  markStarting(taskId: string, terminal: string, agent: string): void {
    if (this.stopped) {
      return
    }
    const until = this.now() + RESTORE_SPINNER_MS
    this.starting.set(`${taskId}/${terminal}`, { taskId, terminal, agent, until })
    this.bus.publish({ kind: 'shells.changed', taskId })
    if (this.poll === undefined) {
      this.poll = setInterval(() => this.checkStarting(), RESTORE_POLL_MS)
      this.poll.unref()
    }
  }

  forget(taskId: string): void {
    this.entries.delete(taskId)
    this.waitingShells.delete(taskId)
    this.held.delete(taskId)
    for (const [key, starting] of this.starting) {
      if (starting.taskId === taskId) {
        this.starting.delete(key)
      }
    }
    const prefix = keyOf(taskId, '')
    for (const key of [...this.active, ...this.probed.keys()]) {
      if (key.startsWith(prefix)) {
        this.active.delete(key)
        this.probed.delete(key)
      }
    }
    for (const timers of [this.exitTimers, this.closing]) {
      for (const key of [...timers.keys()]) {
        if (key.startsWith(prefix)) {
          clearTimer(timers, key)
        }
      }
    }
    deleteOpenShells(this.dataDir, taskId)
  }

  // Before the shutdown closes every shell. After this the list is not written again, so the
  // shells that the shutdown ends stay in it, as after a kill. A quit not yet written down
  // (EXIT_CONFIRM_MS) is not written either.
  stop(): void {
    this.stopped = true
    for (const timer of [...this.exitTimers.values(), ...this.closing.values()]) {
      clearTimeout(timer)
    }
    this.exitTimers.clear()
    this.closing.clear()
    this.unsubscribe?.()
    clearInterval(this.poll)
    this.poll = undefined
  }

  private note(taskId: string, terminal: string, entry: Entry): void {
    if (this.stopped || !this.terminals.isOpen(taskId, terminal)) {
      return
    }
    this.active.add(keyOf(taskId, terminal))
    this.set(taskId, terminal, entry)
    this.watchExit(taskId, terminal)
  }

  private set(taskId: string, terminal: string, entry: Entry): void {
    if (this.stopped || !this.terminals.isOpen(taskId, terminal)) {
      return
    }
    const shells = this.entries.get(taskId) ?? new Map<string, Entry>()
    const old = shells.get(terminal)
    if (old?.agent === entry.agent && old.session === entry.session) {
      return
    }
    shells.set(terminal, entry)
    this.entries.set(taskId, shells)
    this.write(taskId)
  }

  // Keeps the list in step with the open shells: a new shell is added as a plain one, a closed one
  // goes after EXIT_CONFIRM_MS. Shells that wait to come back, or are held, stay.
  private sync(taskId: string): void {
    if (this.stopped) {
      return
    }
    if (this.isEnded(taskId)) {
      if (this.entries.has(taskId)) {
        this.forget(taskId)
      }
      return
    }
    const open = this.terminals.list(taskId)
    const waiting = this.waitingShells.get(taskId)
    const held = this.held.get(taskId)
    // A stage or a person started a shell of a waiting name, or of one that just closed: it is a
    // new shell, not the old one. The stage of a held shell started its agent again.
    const replaced = open.filter((name) => {
      const key = keyOf(taskId, name)
      const closed = this.closing.has(key)
      clearTimer(this.closing, key)
      held?.delete(name)
      return waiting?.delete(name) === true || closed
    })
    for (const name of replaced) {
      this.active.delete(keyOf(taskId, name))
      clearTimer(this.exitTimers, keyOf(taskId, name))
    }
    const old = this.entries.get(taskId) ?? new Map<string, Entry>()
    const kept = [...(waiting ?? []), ...(held ?? [])]
    for (const name of old.keys()) {
      if (!open.includes(name) && !kept.includes(name)) {
        this.closeLater(taskId, name)
      }
    }
    const closing = [...old.keys()].filter((name) => this.closing.has(keyOf(taskId, name)))
    const next = new Map<string, Entry>()
    for (const name of new Set([...open, ...kept, ...closing])) {
      const entry = replaced.includes(name) ? undefined : old.get(name)
      next.set(name, entry ?? PLAIN)
    }
    const same =
      old.size === next.size &&
      [...next].every(([name, entry]) => {
        const before = old.get(name)
        return before?.agent === entry.agent && before.session === entry.session
      })
    if (!same) {
      if (next.size === 0) {
        this.entries.delete(taskId)
      } else {
        this.entries.set(taskId, next)
      }
      this.write(taskId)
    }
    for (const name of open) {
      this.watchExit(taskId, name)
      this.findAgent(taskId, name)
    }
  }

  // A closed shell leaves the list once it stayed closed for EXIT_CONFIRM_MS.
  private closeLater(taskId: string, terminal: string): void {
    const key = keyOf(taskId, terminal)
    if (this.closing.has(key)) {
      return
    }
    clearTimer(this.exitTimers, key)
    const timer = setTimeout(() => {
      this.closing.delete(key)
      this.active.delete(key)
      this.probed.delete(key)
      const shells = this.entries.get(taskId)
      if (this.stopped || shells?.delete(terminal) !== true) {
        return
      }
      if (shells.size === 0) {
        this.entries.delete(taskId)
      }
      this.write(taskId)
    }, EXIT_CONFIRM_MS)
    timer.unref()
    this.closing.set(key, timer)
  }

  // An agent that left the foreground counts as quit once it stayed gone for EXIT_CONFIRM_MS. Just
  // after a launch, and on Windows (no foreground), agentRuns decides, so it looks again.
  private watchExit(taskId: string, terminal: string): void {
    const key = keyOf(taskId, terminal)
    const entry = this.entries.get(taskId)?.get(terminal)
    if (
      this.agentRuns === undefined ||
      !this.active.has(key) ||
      (entry?.agent ?? null) === null ||
      this.inForeground(taskId, terminal)
    ) {
      clearTimer(this.exitTimers, key)
      return
    }
    if (this.exitTimers.has(key)) {
      return
    }
    const timer = setTimeout(() => {
      this.exitTimers.delete(key)
      if (this.stopped || !this.terminals.isOpen(taskId, terminal)) {
        return
      }
      if (this.agentRuns?.(taskId, terminal) === false) {
        this.active.delete(key)
        this.set(taskId, terminal, PLAIN)
      } else {
        this.watchExit(taskId, terminal)
      }
    }, EXIT_CONFIRM_MS)
    timer.unref()
    this.exitTimers.set(key, timer)
  }

  // An agent typed by hand reports no launch, and Claude Code, Codex and Gemini no hook either
  // (their hooks come with Natamux's launch line). Its process names it; with no session it comes
  // back on the newest session of the folder.
  private findAgent(taskId: string, terminal: string): void {
    const key = keyOf(taskId, terminal)
    const program = this.terminals.foreground(taskId, terminal)
    const entry = this.entries.get(taskId)?.get(terminal)
    if (
      this.agentIn === undefined ||
      entry === undefined ||
      entry.agent !== null ||
      program === undefined ||
      !this.inForeground(taskId, terminal)
    ) {
      this.probed.delete(key)
      return
    }
    if (this.probed.get(key) === program) {
      return
    }
    this.probed.set(key, program)
    void this.noteFound(taskId, terminal)
  }

  private async noteFound(taskId: string, terminal: string): Promise<void> {
    let agent: string | null = null
    try {
      agent = (await this.agentIn?.(taskId, terminal)) ?? null
    } catch {
      // The process ended while `ps` ran.
      return
    }
    if (agent !== null && this.entries.get(taskId)?.get(terminal)?.agent === null) {
      this.note(taskId, terminal, { agent, session: null })
    }
  }

  private inForeground(taskId: string, terminal: string): boolean {
    const program = this.terminals.foreground(taskId, terminal)
    return program !== undefined && !SHELLS.includes(basename(program))
  }

  private checkStarting(): void {
    const changed = new Set<string>()
    for (const [key, { taskId, terminal, until }] of this.starting) {
      const program = this.terminals.foreground(taskId, terminal)
      const runs = program !== undefined && !SHELLS.includes(basename(program))
      if (runs || !this.terminals.isOpen(taskId, terminal) || this.now() >= until) {
        this.starting.delete(key)
        changed.add(taskId)
      }
    }
    for (const taskId of changed) {
      this.bus.publish({ kind: 'shells.changed', taskId })
    }
    if (this.starting.size === 0) {
      clearInterval(this.poll)
      this.poll = undefined
    }
  }

  private write(taskId: string): void {
    const shells = [...(this.entries.get(taskId) ?? [])]
      .map(([name, entry]) => ({ name, ...entry }))
      .sort((a, b) => Number(a.name) - Number(b.name))
    try {
      if (shells.length === 0) {
        deleteOpenShells(this.dataDir, taskId)
      } else {
        this.save(this.dataDir, taskId, shells)
      }
    } catch {
      // The list is a convenience: a full disk must not break a shell.
    }
  }
}

// An agent comes back on the session it reported, or with none on the newest of the folder.
function resumable(entry: Entry): entry is { agent: string; session: string | null } {
  return entry.agent !== null && isLaunchable(entry.agent)
}
