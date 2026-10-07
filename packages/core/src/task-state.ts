import { z } from 'zod'

export const TASK_STATES = [
  'draft',
  'ready',
  'running',
  'needs-input',
  'review',
  'blocked',
  'failed',
  'done',
  'cancelled',
] as const

export const taskStateSchema = z.enum(TASK_STATES)
export type TaskState = z.infer<typeof taskStateSchema>

const ALLOWED_TRANSITIONS: Record<TaskState, readonly TaskState[]> = {
  draft: ['ready', 'cancelled'],
  ready: ['running', 'blocked', 'cancelled'],
  running: ['needs-input', 'review', 'failed', 'blocked', 'cancelled'],
  'needs-input': ['running', 'cancelled'],
  review: ['done', 'running', 'cancelled'],
  blocked: ['ready', 'cancelled'],
  failed: ['ready', 'cancelled'],
  done: [],
  cancelled: [],
}

// States the dashboard shows in its "Needs me" strip.
export const NEEDS_ME_STATES: readonly TaskState[] = ['needs-input', 'review', 'failed', 'blocked']

export class InvalidTransitionError extends Error {
  constructor(
    readonly from: TaskState,
    readonly to: TaskState,
  ) {
    super(`Cannot move a task from "${from}" to "${to}"`)
    this.name = 'InvalidTransitionError'
  }
}

export function canTransition(from: TaskState, to: TaskState): boolean {
  return ALLOWED_TRANSITIONS[from].includes(to)
}

export function nextStates(from: TaskState): readonly TaskState[] {
  return ALLOWED_TRANSITIONS[from]
}

export function isTerminalState(state: TaskState): boolean {
  return ALLOWED_TRANSITIONS[state].length === 0
}

export function needsMe(state: TaskState): boolean {
  return NEEDS_ME_STATES.includes(state)
}

export function transition(from: TaskState, to: TaskState): TaskState {
  if (!canTransition(from, to)) {
    throw new InvalidTransitionError(from, to)
  }
  return to
}
