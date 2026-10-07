import { sep } from 'node:path'
import { type EventType, type Task, type TaskState, taskIdSchema } from '@helmsman/core'
import { z } from 'zod'
import { TaskNotFoundError, type TaskService } from './task-service.js'

// The part of the JSON that Claude Code sends to a hook which Helmsman reads.
export const hookInputSchema = z.object({
  hook_event_name: z.string().min(1),
  cwd: z.string().optional(),
  session_id: z.string().optional(),
  notification_type: z.string().optional(),
  message: z.string().optional(),
})

export type HookInput = z.infer<typeof hookInputSchema>

export type PlannedEvent = {
  type: EventType
  payload: Record<string, unknown>
}

export type HookResult = {
  taskId: string | null
  applied: EventType[]
}

const MAX_MESSAGE_LENGTH = 500

// Notification types that mean Claude waits for the user.
const ASKING_NOTIFICATIONS: readonly string[] = [
  'permission_prompt',
  'elicitation_dialog',
  'agent_needs_input',
]

function describe(input: HookInput): Record<string, unknown> {
  return {
    hook: input.hook_event_name,
    ...(input.session_id === undefined ? {} : { sessionId: input.session_id }),
    ...(input.notification_type === undefined ? {} : { notificationType: input.notification_type }),
    ...(input.message === undefined ? {} : { message: input.message.slice(0, MAX_MESSAGE_LENGTH) }),
  }
}

// Brings a task that waits for the user back to running, then adds the closing event.
function endTurn(
  state: TaskState,
  closing: EventType,
  payload: Record<string, unknown>,
): PlannedEvent[] {
  if (state === 'running') {
    return [{ type: closing, payload }]
  }
  if (state === 'needs-input') {
    return [
      { type: 'agent.resumed', payload },
      { type: closing, payload },
    ]
  }
  return []
}

// Decides which task events a hook means for a task in this state. An empty list means "ignore it",
// so the event log does not fill up with calls that change nothing.
export function planHookEvents(state: TaskState, input: HookInput): PlannedEvent[] {
  const payload = describe(input)

  switch (input.hook_event_name) {
    case 'UserPromptSubmit':
      if (state === 'draft') {
        return [
          { type: 'task.readied', payload },
          { type: 'agent.started', payload },
        ]
      }
      if (state === 'ready') {
        return [{ type: 'agent.started', payload }]
      }
      return state === 'needs-input' || state === 'review'
        ? [{ type: 'agent.resumed', payload }]
        : []

    case 'PostToolUse':
      // A tool ran, so a permission that was asked for has been answered.
      return state === 'needs-input' ? [{ type: 'agent.resumed', payload }] : []

    case 'PermissionRequest':
      return state === 'running' ? [{ type: 'agent.asked', payload }] : []

    case 'Notification':
      return state === 'running' &&
        input.notification_type !== undefined &&
        ASKING_NOTIFICATIONS.includes(input.notification_type)
        ? [{ type: 'agent.asked', payload }]
        : []

    case 'Stop':
      return endTurn(state, 'agent.finished', payload)

    case 'StopFailure':
      return endTurn(state, 'agent.failed', payload)

    default:
      return []
  }
}

function isInside(folder: string, path: string): boolean {
  return path === folder || path.startsWith(folder.endsWith(sep) ? folder : `${folder}${sep}`)
}

export class ClaudeCodeHookService {
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

  handle(input: HookInput, taskIdHeader: string | undefined): HookResult {
    const task = this.findTask(input.cwd, taskIdHeader)
    if (!task) {
      return { taskId: null, applied: [] }
    }

    const applied: EventType[] = []
    for (const planned of planHookEvents(task.state, input)) {
      this.tasks.applyEvent(task.id, {
        source: 'agent',
        type: planned.type,
        payload: planned.payload,
      })
      applied.push(planned.type)
    }
    return { taskId: task.id, applied }
  }
}
