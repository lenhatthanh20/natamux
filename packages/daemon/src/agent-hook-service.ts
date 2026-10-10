import { isAbsolute, relative, sep } from 'node:path'
import { type EventType, type Task, type TaskState, taskIdSchema } from '@natamux/core'
import { findAdapter, knownAgentIds } from './agent-adapters.js'
import { planSignalEvents } from './agent-signals.js'
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

// What the hook service needs to know about stages: which stage a terminal works on, and whether
// a turn end in a terminal follows a report (see StageRunner).
export type StageLookup = {
  laneFor: (task: Task, terminal: string | undefined) => { stage: string; state: TaskState }
  takeReport: (taskId: string, terminal: string) => boolean
}

const NO_STAGES: StageLookup = {
  laneFor: (task) => ({ stage: task.stage, state: task.state }),
  takeReport: () => false,
}

export class AgentHookService {
  private readonly tasks: TaskService
  private readonly stages: StageLookup
  // What the agent of each task does now. Kept in memory only: it is gone after a restart, and
  // a new hook fills it again.
  private readonly activities = new Map<string, AgentActivity>()

  constructor(tasks: TaskService, stages: StageLookup = NO_STAGES) {
    this.tasks = tasks
    this.stages = stages
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

  activity(): Record<string, AgentActivity> {
    return Object.fromEntries(this.activities)
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
      this.activities.delete(task.id)
      return { taskId: null, applied: [] }
    }

    if (hook.summary !== undefined) {
      this.activities.set(task.id, { text: hook.summary, at: new Date().toISOString() })
    }
    if (hook.signal === 'turn-ended' || hook.signal === 'failed' || hook.signal === 'asking') {
      this.activities.delete(task.id)
    }

    const lane = this.stages.laneFor(task, terminal)
    // The turn in which the agent ran `natamux done` ends as planned, not "stopped early".
    const reportedTurn =
      hook.signal === 'turn-ended' && this.stages.takeReport(task.id, terminal ?? MAIN_TERMINAL)
    if (reportedTurn) {
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
}
