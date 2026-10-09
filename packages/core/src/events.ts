import { z } from 'zod'
import {
  combinedState,
  enterStep,
  firstPosition,
  isGroup,
  type Stage,
  type StagePosition,
  type Step,
  stagesOf,
  stepIndex,
} from './stage.js'
import { isTerminalState, type TaskState, taskStateSchema } from './task-state.js'

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
  'review.commented',
  'stage.started',
  'stage.reported',
  'stage.check-failed',
  'stage.checked',
  'stage.approved',
  'stage.passed',
  'stage.failed',
  'stage.reopened',
  'stage.skipped',
  'context.fetched',
  'issue.linked',
  'issue.created',
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
  'review.commented',
  'context.fetched',
  'issue.linked',
  'issue.created',
]

// A move of one stage: the states it may happen in, and the state it leads to. `finish` ends the
// stage; `gate` ends it when its gate is auto and waits for approval when it is manual.
type Move = { from: readonly TaskState[]; to: TaskState | 'finish' | 'gate' }

const MOVES: Partial<Record<EventType, Move>> = {
  'task.readied': { from: ['draft'], to: 'ready' },
  'task.blocked': { from: ['ready', 'running'], to: 'blocked' },
  'task.unblocked': { from: ['blocked'], to: 'ready' },
  'task.retried': { from: ['failed'], to: 'ready' },
  'agent.started': { from: ['ready'], to: 'running' },
  'stage.started': { from: ['draft', 'ready'], to: 'running' },
  'agent.asked': { from: ['running'], to: 'needs-input' },
  // A prompt typed to an agent whose stage waits for approval takes the stage up again.
  'agent.resumed': { from: ['needs-input', 'review'], to: 'running' },
  // The agent ended its turn without saying the stage is done (`helmsman done`), for example to
  // ask something in plain text. The daemon drops the turn end that follows a report.
  'agent.finished': { from: ['running'], to: 'needs-input' },
  'agent.failed': { from: ['running'], to: 'failed' },
  // "Mark as done" from needs-input runs the checks again.
  'stage.reported': { from: ['running', 'needs-input'], to: 'running' },
  'stage.checked': { from: ['running'], to: 'gate' },
  // Approve anyway: a stage whose checks failed twice can still be approved.
  'stage.approved': { from: ['review', 'needs-input'], to: 'finish' },
  'review.approved': { from: ['review', 'needs-input'], to: 'finish' },
  'stage.passed': { from: ['ready', 'running', 'review'], to: 'finish' },
  'stage.failed': { from: ['running'], to: 'failed' },
  'stage.skipped': {
    from: ['draft', 'ready', 'running', 'needs-input', 'review', 'blocked', 'failed'],
    to: 'finish',
  },
}

export type AppliedEvent = {
  position: StagePosition
  accepted: boolean
  // The step changed: the task entered a new stage or group, or went back to one.
  moved: boolean
}

function payloadString(event: TaskEvent, key: string): string | undefined {
  const value = event.payload[key]
  return typeof value === 'string' && value !== '' ? value : undefined
}

// The step to go back to: `to`, else the last agent stage before the current step.
function reopenTarget(steps: readonly Step[], current: number, to: string | undefined) {
  if (to !== undefined) {
    const index = stepIndex(steps, to)
    return index >= 0 && index <= current ? { index, stage: to } : undefined
  }
  for (let index = current - 1; index >= 0; index -= 1) {
    const step = steps[index]
    const stage = step === undefined ? undefined : agentStageOf(step)
    if (stage !== undefined) {
      return { index, stage: stage.id }
    }
  }
  return undefined
}

// The stage of a step that changes code: an agent stage with no output file.
function agentStageOf(step: Step): Stage | undefined {
  const agents = stagesOf(step).filter((stage) => stage.kind === 'agent')
  return agents.find((stage) => stage.output === undefined) ?? agents[0]
}

function reopen(steps: readonly Step[], index: number, stage: string, state: TaskState) {
  const step = steps[index]
  if (step === undefined) {
    return undefined
  }
  if (!isGroup(step)) {
    return { stage: step.id, state, lanes: {} }
  }
  const lanes = Object.fromEntries(
    step.parallel.map((lane) => [lane.id, lane.id === stage || step.id === stage ? state : 'done']),
  ) as Record<string, TaskState>
  return { stage: step.id, state: combinedState(lanes), lanes }
}

// After a stage ends: the next step, or done after the last one.
function finishLane(steps: readonly Step[], index: number, position: StagePosition, lane: string) {
  const step = steps[index]
  if (step !== undefined && isGroup(step)) {
    const lanes = { ...position.lanes, [lane]: 'done' as TaskState }
    if (Object.values(lanes).some((state) => state !== 'done')) {
      return { position: { ...position, lanes, state: combinedState(lanes) }, moved: false }
    }
  }
  const next = steps[index + 1]
  if (next === undefined) {
    return { position: { ...position, state: 'done' as TaskState, lanes: {} }, moved: false }
  }
  return { position: enterStep(next), moved: true }
}

// Applies one event to a task's position in its steps. An event that does not fit is not
// accepted and leaves the position as it is.
export function applyTaskEvent(
  position: StagePosition,
  event: TaskEvent,
  steps: readonly Step[],
): AppliedEvent {
  const keep = (accepted: boolean): AppliedEvent => ({ position, accepted, moved: false })
  if (RECORD_ONLY_EVENT_TYPES.includes(event.type)) {
    return keep(true)
  }

  const index = stepIndex(steps, position.stage)
  const step = steps[index]

  if (event.type === 'state.overridden') {
    const target = taskStateSchema.safeParse(event.payload.to)
    if (!target.success) {
      return keep(false)
    }
    const stage = payloadString(event, 'stage')
    if (stage !== undefined) {
      const at = stepIndex(steps, stage)
      const moved = at >= 0 ? reopen(steps, at, stage, target.data) : undefined
      return moved === undefined ? keep(false) : { position: moved, accepted: true, moved: true }
    }
    if (step !== undefined && isGroup(step)) {
      const lanes = Object.fromEntries(
        Object.entries(position.lanes).map(([id, state]) => [
          id,
          state === 'done' ? state : target.data,
        ]),
      ) as Record<string, TaskState>
      return { position: { ...position, lanes, state: target.data }, accepted: true, moved: false }
    }
    return { position: { ...position, state: target.data }, accepted: true, moved: false }
  }

  if (isTerminalState(position.state)) {
    return keep(false)
  }

  if (event.type === 'task.cancelled') {
    return { position: { ...position, state: 'cancelled' }, accepted: true, moved: false }
  }

  if (event.type === 'pr.merged') {
    // A merged pull request finishes a task that is at review or later.
    const firstReview = steps.findIndex((candidate) =>
      stagesOf(candidate).some((stage) => stage.kind === 'review' || stage.kind === 'release'),
    )
    if (firstReview < 0 || index < firstReview) {
      return keep(false)
    }
    const last = steps.at(-1)
    return {
      position: { stage: last?.id ?? position.stage, state: 'done', lanes: {} },
      accepted: true,
      moved: last !== undefined && last.id !== position.stage,
    }
  }

  if (step === undefined) {
    return keep(false)
  }

  // A prompt typed to the agent during review is a request for changes.
  const resumedInReview = event.type === 'agent.resumed' && !isGroup(step) && step.kind === 'review'
  if (
    event.type === 'stage.reopened' ||
    event.type === 'review.changes-requested' ||
    resumedInReview
  ) {
    // Changes asked in review go to the agent as its next prompt, so its stage runs at once.
    const state: TaskState = event.type === 'stage.reopened' ? 'ready' : 'running'
    const target = reopenTarget(steps, index, payloadString(event, 'to'))
    const moved =
      target === undefined ? undefined : reopen(steps, target.index, target.stage, state)
    return moved === undefined ? keep(false) : { position: moved, accepted: true, moved: true }
  }

  // A failed check keeps the stage running while the agent fixes it; the last try waits for you.
  const move: Move | undefined =
    event.type === 'stage.check-failed'
      ? { from: ['running'], to: event.payload.final === true ? 'needs-input' : 'running' }
      : MOVES[event.type]
  if (move === undefined) {
    return keep(false)
  }

  // Inside a group an event names its stage; outside one it may name the current stage only.
  const named = payloadString(event, 'stage')
  const group = isGroup(step)
  const lane = group ? named : (named ?? step.id)
  const stage = stagesOf(step).find((candidate) => candidate.id === lane)
  if (lane === undefined || stage === undefined) {
    return keep(false)
  }
  const current = group ? position.lanes[lane] : position.state
  if (current === undefined || !move.from.includes(current)) {
    return keep(false)
  }

  const to = move.to === 'gate' ? (stage.gate === 'auto' ? 'finish' : 'review') : move.to
  if (to === 'finish') {
    const finished = finishLane(steps, index, position, lane)
    return { position: finished.position, accepted: true, moved: finished.moved }
  }
  if (!group) {
    return { position: { ...position, state: to }, accepted: true, moved: false }
  }
  const lanes = { ...position.lanes, [lane]: to }
  return {
    position: { ...position, lanes, state: combinedState(lanes) },
    accepted: true,
    moved: false,
  }
}

export type DerivedTaskState = {
  position: StagePosition
  rejected: TaskEvent[]
}

// Events are applied in the given order, from the first step. Events that are not accepted are
// returned in `rejected`, never thrown.
export function deriveTaskState(events: readonly TaskEvent[], steps: readonly Step[]) {
  let position = firstPosition(steps)
  const rejected: TaskEvent[] = []

  for (const event of events) {
    const applied = applyTaskEvent(position, event, steps)
    position = applied.position
    if (!applied.accepted) {
      rejected.push(event)
    }
  }

  return { position, rejected } satisfies DerivedTaskState
}
