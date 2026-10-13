import { isAbsolute, relative, sep } from 'node:path'
import { type EventType, type Task, type TaskState, taskIdSchema } from '@natamux/core'
import { findAdapter, knownAgentIds } from './agent-adapters.js'
import { type AgentSignal, type NormalizedHook, planSignalEvents } from './agent-signals.js'
import type { EventBus } from './event-bus.js'
import { TaskNotFoundError, type TaskService } from './task-service.js'
import { MAIN_TERMINAL } from './terminal-service.js'

export type HookResult = {
  taskId: string | null
  applied: EventType[]
}

export class UnknownAgentError extends Error {
  readonly agentId: string

  constructor(agentId: string) {
    super(`Unknown agent "${agentId}". Known agents: ${knownAgentIds().join(', ')}`)
    this.name = 'UnknownAgentError'
    this.agentId = agentId
  }
}

// path.relative also handles mixed slashes and the case of drive letters on Windows.
function isInside(folder: string, path: string): boolean {
  const rest = relative(folder, path)
  return rest === '' || (rest !== '..' && !rest.startsWith(`..${sep}`) && !isAbsolute(rest))
}

export type AgentActivity = { text: string; at: string }

// What the hook service needs to know about stages: which stage a terminal works on (null for
// none), and whether a turn end in a terminal follows a report (see StageRunner).
export type StageLookup = {
  laneFor: (task: Task, terminal: string | undefined) => { stage: string; state: TaskState } | null
  takeReport: (taskId: string, terminal: string) => boolean
}

// The state of an agent in a shell that no stage uses. A turn end waits for the user, as it does
// for the task's own agent.
const SHELL_STATES: Record<AgentSignal, TaskState> = {
  prompt: 'running',
  activity: 'running',
  asking: 'needs-input',
  'turn-paused': 'running',
  'turn-ended': 'needs-input',
  failed: 'failed',
}

const NO_STAGES: StageLookup = {
  laneFor: (task) => ({ stage: task.stage, state: task.state }),
  takeReport: () => false,
}

// Only a tool that ran, or the end of a turn, can have changed files. A tool that is about to run
// (PreToolUse, beforeShellExecution, BeforeTool) changed nothing yet.
function changesFiles(hook: NormalizedHook): boolean {
  if (hook.signal === 'activity') {
    const name = hook.details.hook
    return typeof name !== 'string' || !/^(pre|before)/i.test(name)
  }
  return hook.signal === 'turn-ended' || hook.signal === 'turn-paused' || hook.signal === 'failed'
}

type HookServiceOptions = {
  // Dashboards hear of each change of the activity, and of the state of an agent in a shell.
  bus?: EventBus
  // Called after a hook that may have changed files in the task's workspace.
  workspaceChanged?: (taskId: string) => void
}

export class AgentHookService {
  private readonly tasks: TaskService
  private readonly stages: StageLookup
  // What the agent in each terminal of each task does now, by task id, then terminal. Kept in
  // memory only: it is gone after a restart, and a new hook fills it again.
  private readonly activities = new Map<string, Map<string, AgentActivity>>()
  // The state of each agent in a shell that no stage uses, by task id, then terminal. In memory
  // only, like the activities.
  private readonly shellStates = new Map<string, Map<string, TaskState>>()
  // "<task>/<terminal>" of the shells whose state the last read of the task's shells dropped, as
  // no agent runs there for it (see forgetShellsExcept). No dashboard shows their state, so a hook
  // there sends no shells.changed: each one would only read the shells, and drop it, again.
  private readonly hiddenShells = new Set<string>()
  private readonly bus: EventBus | undefined
  private readonly workspaceChanged: (taskId: string) => void

  constructor(
    tasks: TaskService,
    stages: StageLookup = NO_STAGES,
    options: HookServiceOptions = {},
  ) {
    this.tasks = tasks
    this.stages = stages
    this.bus = options.bus
    this.workspaceChanged = options.workspaceChanged ?? (() => undefined)
  }

  // The task comes from the header when it names one, otherwise from the workspace that holds `cwd`.
  findTask(cwd: string | undefined, taskIdHeader: string | undefined): Task | undefined {
    if (taskIdHeader !== undefined && taskIdSchema.safeParse(taskIdHeader).success) {
      try {
        return this.tasks.getTask(taskIdHeader)
      } catch (error) {
        if (!(error instanceof TaskNotFoundError)) {
          throw error
        }
      }
    }
    if (cwd === undefined) {
      return undefined
    }
    return this.tasks
      .listTasks()
      .find((task) => task.workspace !== null && isInside(task.workspace, cwd))
  }

  activity(): Record<string, Record<string, AgentActivity>> {
    return Object.fromEntries(
      [...this.activities].map(([taskId, lines]) => [taskId, Object.fromEntries(lines)]),
    )
  }

  // Throws UnknownAgentError for an agent without an adapter, and a ZodError when the input is not
  // a hook of that agent.
  handle(
    agentId: string,
    raw: unknown,
    taskIdHeader: string | undefined,
    terminal?: string,
  ): HookResult {
    const adapter = findAdapter(agentId)
    if (!adapter) {
      throw new UnknownAgentError(agentId)
    }

    const hook = adapter.normalize(raw)
    if (!hook) {
      return { taskId: null, applied: [] }
    }

    const task = this.findTask(hook.cwd, taskIdHeader)
    if (!task) {
      return { taskId: null, applied: [] }
    }
    // Natamux terminals run in the task's workspace only. Without one, the hook comes from an agent
    // that the removal of the workspace ended.
    if (task.workspace === null && terminal !== undefined) {
      this.keepActivity(task.id, new Map(), this.activities.has(task.id))
      return { taskId: null, applied: [] }
    }

    this.noteActivity(task.id, terminal ?? MAIN_TERMINAL, hook)
    if (changesFiles(hook)) {
      this.workspaceChanged(task.id)
    }

    const lane = this.stages.laneFor(task, terminal)
    // The turn in which the agent ran `natamux done`, or sent the task back, ends as planned, not
    // "stopped early", even when background work is still running.
    const reportedTurn =
      (hook.signal === 'turn-ended' || hook.signal === 'turn-paused') &&
      this.stages.takeReport(task.id, terminal ?? MAIN_TERMINAL)
    if (reportedTurn) {
      this.forgetActivity(task.id, terminal ?? MAIN_TERMINAL)
      return { taskId: task.id, applied: [] }
    }
    if (lane === null) {
      // An agent on no stage of the step: one the user started by hand, or a reviewer that sent
      // the task back. It has a state of its own and leaves the task alone.
      const states = this.shellStates.get(task.id) ?? new Map<string, TaskState>()
      const state = SHELL_STATES[hook.signal]
      const shell = terminal ?? MAIN_TERMINAL
      if (states.get(shell) !== state) {
        states.set(shell, state)
        this.shellStates.set(task.id, states)
        // The dashboards show this state with the task's shells.
        if (!this.hiddenShells.has(`${task.id}/${shell}`)) {
          this.bus?.publish({ kind: 'shells.changed', taskId: task.id })
        }
      }
      return { taskId: task.id, applied: [] }
    }
    const applied: EventType[] = []
    for (const planned of planSignalEvents(lane.state, hook.signal, hook.details)) {
      this.tasks.applyEvent(task.id, {
        source: 'agent',
        type: planned.type,
        payload: { ...planned.payload, stage: lane.stage },
      })
      applied.push(planned.type)
    }

    // A hook that changes nothing still shows that a running agent is alive.
    if (applied.length === 0) {
      this.tasks.recordActivity(task.id)
    }
    return { taskId: task.id, applied }
  }

  shellState(taskId: string, terminal: string): TaskState | undefined {
    return this.shellStates.get(taskId)?.get(terminal)
  }

  // Called with the shells where an agent runs now. An agent started later in another shell must
  // not show the state of one that ended there.
  forgetShellsExcept(taskId: string, running: readonly string[]): void {
    const states = this.shellStates.get(taskId)
    for (const terminal of states?.keys() ?? []) {
      if (!running.includes(terminal)) {
        states?.delete(terminal)
        this.hiddenShells.add(`${taskId}/${terminal}`)
      }
    }
    for (const terminal of running) {
      this.hiddenShells.delete(`${taskId}/${terminal}`)
    }
    if (states?.size === 0) {
      this.shellStates.delete(taskId)
    }
  }

  // Each terminal keeps its own line, so the end of a turn in one does not clear the other.
  private noteActivity(taskId: string, terminal: string, hook: NormalizedHook): void {
    const lines = this.activities.get(taskId) ?? new Map<string, AgentActivity>()
    let changed = false
    if (hook.summary !== undefined) {
      lines.set(terminal, { text: hook.summary, at: new Date().toISOString() })
      changed = true
    }
    if (hook.signal === 'turn-ended' || hook.signal === 'failed' || hook.signal === 'asking') {
      changed = lines.delete(terminal) || changed
    }
    this.keepActivity(taskId, lines, changed)
  }

  private forgetActivity(taskId: string, terminal: string): void {
    const lines = this.activities.get(taskId) ?? new Map<string, AgentActivity>()
    this.keepActivity(taskId, lines, lines.delete(terminal))
  }

  private keepActivity(taskId: string, lines: Map<string, AgentActivity>, changed: boolean): void {
    if (lines.size === 0) {
      this.activities.delete(taskId)
    } else {
      this.activities.set(taskId, lines)
    }
    if (changed) {
      const activity = Object.fromEntries(lines)
      this.bus?.publish({ kind: 'activity.changed', taskId, activity })
    }
  }
}
