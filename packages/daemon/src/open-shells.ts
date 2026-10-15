import { mkdirSync, readFileSync, renameSync, rmSync, writeFileSync } from 'node:fs'
import { basename, join } from 'node:path'
import { z } from 'zod'
import { isLaunchable } from './agent-launch.js'
import type { EventBus } from './event-bus.js'
import { tasksDir } from './task-files.js'
import { SHELLS, type TerminalService } from './terminal-service.js'

// A shell that was open, so it comes back after a restart: the agent that ran in it and the
// last session that agent reported. Null: a plain shell, or no session yet.
const openShellSchema = z.object({
  name: z.string().regex(/^[1-9]\d{0,2}$/),
  agent: z.string().min(1).nullable(),
  session: z.string().min(1).nullable(),
})

export type OpenShell = z.infer<typeof openShellSchema>

const openShellsSchema = z.array(openShellSchema)

// A restored or restarted agent shows a spinner until it is in the foreground, at most this long.
export const RESTORE_SPINNER_MS = 15_000
export const RESTORE_POLL_MS = 500

export function openShellsPath(dataDir: string, taskId: string): string {
  return join(tasksDir(dataDir), `${taskId}.open-shells.json`)
}

export function saveOpenShells(dataDir: string, taskId: string, shells: OpenShell[]): void {
  mkdirSync(tasksDir(dataDir), { recursive: true })
  const path = openShellsPath(dataDir, taskId)
  const temporaryPath = `${path}.tmp`
  writeFileSync(temporaryPath, JSON.stringify(shells))
  renameSync(temporaryPath, path)
}

// A missing or broken file reads as no shells: nothing comes back, the daemon still starts.
export function readOpenShells(dataDir: string, taskId: string): OpenShell[] {
  try {
    const text = readFileSync(openShellsPath(dataDir, taskId), 'utf8')
    return openShellsSchema.parse(JSON.parse(text))
  } catch {
    return []
  }
}

export function deleteOpenShells(dataDir: string, taskId: string): void {
  rmSync(openShellsPath(dataDir, taskId), { force: true })
}

type Entry = { agent: string | null; session: string | null }
type Starting = { taskId: string; terminal: string; agent: string; until: number }

type OpenShellsOptions = {
  dataDir: string
  terminals: Pick<TerminalService, 'list' | 'isOpen' | 'foreground'>
  bus: EventBus
  // True for a task that is done, cancelled or gone: it keeps no shells.
  isEnded: (taskId: string) => boolean
  now?: () => number
  save?: (dataDir: string, taskId: string, shells: OpenShell[]) => void
}

// The open shells of each task, kept in a file beside the task as they change, so a crash or a
// kill without SIGTERM (Windows) loses nothing. After a restart, the shells that no stage
// restarts wait until a dashboard attaches to them (see StageRunner.restoreShell).
export class OpenShells {
  private readonly dataDir: string
  private readonly terminals: OpenShellsOptions['terminals']
  private readonly bus: EventBus
  private readonly isEnded: (taskId: string) => boolean
  private readonly now: () => number
  private readonly save: NonNullable<OpenShellsOptions['save']>
  private readonly entries = new Map<string, Map<string, Entry>>()
  // Shells from the file that have not started again yet, by task.
  private readonly waitingShells = new Map<string, Set<string>>()
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
  // starts again at once) do not wait for a dashboard.
  load(taskId: string, skip: readonly string[]): void {
    const shells = readOpenShells(this.dataDir, taskId)
    if (shells.length === 0) {
      return
    }
    this.entries.set(
      taskId,
      new Map(shells.map(({ name, agent, session }) => [name, { agent, session }])),
    )
    const waiting = shells
      .map((shell) => shell.name)
      .filter((name) => !skip.includes(name) && !this.terminals.isOpen(taskId, name))
    if (waiting.length > 0) {
      this.waitingShells.set(taskId, new Set(waiting))
    }
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
    this.set(taskId, terminal, { agent, session: session ?? null })
  }

  // A hook named the session of the agent in a shell.
  noteSession(taskId: string, terminal: string, agent: string, session: string): void {
    this.set(taskId, terminal, { agent, session })
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
    for (const [key, starting] of this.starting) {
      if (starting.taskId === taskId) {
        this.starting.delete(key)
      }
    }
    deleteOpenShells(this.dataDir, taskId)
  }

  // Before the shutdown closes every shell. A shell whose agent the person quit comes back as a
  // plain shell. After this the list is not written again, so the shells that the shutdown ends
  // stay in it.
  stop(agentRuns: (taskId: string, terminal: string) => boolean): void {
    for (const [taskId, shells] of this.entries) {
      let changed = false
      for (const [name, entry] of shells) {
        const open = this.terminals.isOpen(taskId, name)
        if (open && entry.agent !== null && !agentRuns(taskId, name)) {
          shells.set(name, { agent: null, session: null })
          changed = true
        }
      }
      if (changed) {
        this.write(taskId)
      }
    }
    this.stopped = true
    this.unsubscribe?.()
    clearInterval(this.poll)
    this.poll = undefined
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
  // goes. Shells that wait to come back stay.
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
    // A stage or a person started a shell of a waiting name: it is a new shell, not the old one.
    const replaced = open.filter((name) => waiting?.delete(name) === true)
    const old = this.entries.get(taskId) ?? new Map<string, Entry>()
    const next = new Map<string, Entry>()
    for (const name of [...open, ...(waiting ?? [])]) {
      const kept = replaced.includes(name) ? undefined : old.get(name)
      next.set(name, kept ?? { agent: null, session: null })
    }
    const same =
      old.size === next.size &&
      [...next].every(([name, entry]) => {
        const before = old.get(name)
        return before?.agent === entry.agent && before.session === entry.session
      })
    if (same) {
      return
    }
    if (next.size === 0) {
      this.entries.delete(taskId)
    } else {
      this.entries.set(taskId, next)
    }
    this.write(taskId)
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

// An agent comes back only on a session it reported. Without one it would start a new session,
// so the shell comes back plain instead.
function resumable(entry: Entry): entry is { agent: string; session: string } {
  return entry.agent !== null && isLaunchable(entry.agent) && entry.session !== null
}
