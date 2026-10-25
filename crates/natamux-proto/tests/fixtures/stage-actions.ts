import {
  type EventType,
  isGroup,
  type Stage,
  type Step,
  type Task,
  type TaskEvent,
  type TaskState,
} from '@natamux/core'

// Events of an agent that leave it where it was: still at the end of its turn.
export const KEEPS_TURN: EventType[] = ['agent.active']

export const STAGE_ACTIONS = [
  'approve',
  'reopen',
  'skip',
  'pass',
  'fail',
  'report',
  'start',
] as const
export type StageAction = (typeof STAGE_ACTIONS)[number]

export type StageActionInput = {
  // The stage the action is about. Needed inside a group when more than one stage could be meant.
  stage?: string
  // The terminal the action came from (`natamux done` in an agent's terminal).
  terminal?: string
  // reopen: the stage to go back to.
  to?: string
  // reopen and fail: why, for the agent and the log.
  reason?: string
  // The request came with the agent token of a task shell.
  byAgent?: boolean
}

export class StageActionError extends Error {
  constructor(message: string) {
    super(message)
    this.name = 'StageActionError'
  }
}

// An action that names something the task does not have, such as a stage to go back to.
export class StageInputError extends Error {
  constructor(message: string) {
    super(message)
    this.name = 'StageInputError'
  }
}

// An event as a client or the runner sends it, before the task service logs it.
export type EventInput = Pick<TaskEvent, 'source' | 'type' | 'payload'>

// The states of a stage whose agent starts again when the daemon starts.
export const RESUMED_STATES: readonly TaskState[] = ['running', 'needs-input', 'review']

export function laneState(
  task: Pick<Task, 'state' | 'lanes'>,
  step: Step,
  stageId: string,
): TaskState {
  return isGroup(step) ? (task.lanes[stageId] ?? 'done') : task.state
}

// Events that end a stage and so may move the task on to the next step. A send back does not
// count: a person who sends the task back to a stage marked to skip wants it to run.
export const ENDS_STAGE: readonly TaskEvent['type'][] = [
  'stage.approved',
  'review.approved',
  'stage.passed',
  'stage.checked',
  'stage.skipped',
]

// Rounds of "fix it" an agent may ask for on its own before a person decides.
export const MAX_AGENT_SEND_BACKS = 2

// A stage an agent works on: an agent stage, or a testing stage with no command.
export function runByAgent(stage: Stage): boolean {
  return stage.kind === 'agent' || (stage.kind === 'testing' && stage.command === undefined)
}

// Why a send back asked for changes, for the agent's next prompt.
export function sendBackReason(cause: TaskEvent | undefined): string | undefined {
  if (
    cause === undefined ||
    (cause.type !== 'stage.reopened' && cause.type !== 'review.changes-requested')
  ) {
    return undefined
  }
  const reason = cause.payload.reason
  return typeof reason === 'string' && reason !== '' ? reason : undefined
}
