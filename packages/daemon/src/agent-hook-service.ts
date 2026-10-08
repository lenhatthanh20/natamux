import { sep } from 'node:path'
import { type EventType, type Task, taskIdSchema } from '@helmsman/core'
import { findAdapter, knownAgentIds } from './agent-adapters.js'
import { planSignalEvents } from './agent-signals.js'
import { TaskNotFoundError, type TaskService } from './task-service.js'

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

function isInside(folder: string, path: string): boolean {
  return path === folder || path.startsWith(folder.endsWith(sep) ? folder : `${folder}${sep}`)
}

export class AgentHookService {
  private readonly tasks: TaskService

  constructor(tasks: TaskService) {
    this.tasks = tasks
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

  // Throws UnknownAgentError for an agent without an adapter, and a ZodError when the input is not
  // a hook of that agent.
  handle(agentId: string, raw: unknown, taskIdHeader: string | undefined): HookResult {
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

    const applied: EventType[] = []
    for (const planned of planSignalEvents(task.state, hook.signal, hook.details)) {
      this.tasks.applyEvent(task.id, {
        source: 'agent',
        type: planned.type,
        payload: planned.payload,
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
