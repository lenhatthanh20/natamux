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
  'workspace.created',
  'workspace.removed',
  'agent.active',
  'git.committed',
  'git.pushed',
  'pr.opened',
  'agent.launched',
  'task.edited',
  'pr.merged',
  'pr.closed',
  'ci.passed',
  'ci.failed',
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
  // A merged pull request finishes a task that waits for review.
  'pr.merged': 'done',
}

export type DerivedTaskState = {
  state: TaskState
  rejected: TaskEvent[]
}

// Events the daemon records without a change of state.
export const RECORD_ONLY_EVENT_TYPES: readonly EventType[] = [
  'task.created',
  'workspace.created',
  'workspace.removed',
  'agent.active',
  'git.committed',
  'git.pushed',
  'pr.opened',
  'agent.launched',
  'task.edited',
  'pr.closed',
  'ci.passed',
  'ci.failed',
]

export type AppliedEvent = {
  state: TaskState
  accepted: boolean
}

// An event that does not fit the current state is not accepted and leaves the state as it is.
export function applyTaskEvent(state: TaskState, event: TaskEvent): AppliedEvent {
  if (RECORD_ONLY_EVENT_TYPES.includes(event.type)) {
    return { state, accepted: true }
  }

  if (event.type === 'state.overridden') {
    const target = taskStateSchema.safeParse(event.payload.to)
    return target.success ? { state: target.data, accepted: true } : { state, accepted: false }
  }

  const target = TARGET_STATE[event.type]
  if (target !== undefined && canTransition(state, target)) {
    return { state: target, accepted: true }
  }
  return { state, accepted: false }
}

// Events are applied in the given order. Events that are not accepted are
// returned in `rejected`, never thrown.
export function deriveTaskState(events: readonly TaskEvent[]): DerivedTaskState {
  let state: TaskState = 'draft'
  const rejected: TaskEvent[] = []

  for (const event of events) {
    const applied = applyTaskEvent(state, event)
    state = applied.state
    if (!applied.accepted) {
      rejected.push(event)
    }
  }

  return { state, rejected }
}
