import { z } from 'zod'
import { canTransition, type TaskState, taskStateSchema } from './task-state.js'

export const EVENT_SOURCES = ['agent', 'human', 'git', 'ci', 'system'] as const

export const EVENT_TYPES = [
  'task.created',
  'task.readied',
  'task.blocked',
  'task.unblocked',
  'task.retried',
  'task.cancelled',
  'agent.started',
  'agent.asked',
  'agent.resumed',
  'agent.finished',
  'agent.failed',
  'review.approved',
  'review.changes-requested',
  'state.overridden',
] as const

export const taskEventSchema = z.object({
  taskId: z.string().min(1),
  source: z.enum(EVENT_SOURCES),
  type: z.enum(EVENT_TYPES),
  at: z.iso.datetime(),
  payload: z.record(z.string(), z.unknown()).default({}),
})

export type TaskEvent = z.infer<typeof taskEventSchema>
export type EventType = TaskEvent['type']

const TARGET_STATE: Partial<Record<EventType, TaskState>> = {
  'task.readied': 'ready',
  'task.blocked': 'blocked',
  'task.unblocked': 'ready',
  'task.retried': 'ready',
  'task.cancelled': 'cancelled',
  'agent.started': 'running',
  'agent.asked': 'needs-input',
  'agent.resumed': 'running',
  'agent.finished': 'review',
  'agent.failed': 'failed',
  'review.approved': 'done',
  'review.changes-requested': 'running',
}

export type DerivedTaskState = {
  state: TaskState
  rejected: TaskEvent[]
}

// Events are applied in the given order. An event that does not fit the
// current state is skipped and returned in `rejected`, never thrown.
export function deriveTaskState(events: readonly TaskEvent[]): DerivedTaskState {
  let state: TaskState = 'draft'
  const rejected: TaskEvent[] = []

  for (const event of events) {
    if (event.type === 'task.created') {
      continue
    }

    if (event.type === 'state.overridden') {
      const target = taskStateSchema.safeParse(event.payload.to)
      if (target.success) {
        state = target.data
      } else {
        rejected.push(event)
      }
      continue
    }

    const target = TARGET_STATE[event.type]
    if (target !== undefined && canTransition(state, target)) {
      state = target
    } else {
      rejected.push(event)
    }
  }

  return { state, rejected }
}
