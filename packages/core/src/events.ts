import { z } from 'zod'
import {
  combinedState,
  enterStep,
  entryState,
  firstPosition,
  isBranch,
  isGroup,
  isNotTaken,
  isStage,
  pathOf,
  type Stage,
  type StageBranch,
  type StagePosition,
  type Step,
  stageOfTerminal,
  stagesOf,
  stepIndex,
  stepToRun,
  takenStages,
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
  'agent.waiting',
  'agent.stopped',
  'attachment.added',
  'stage.workflow-set',
  'stage.ci-set',
  'stage.ci-started',
  'stage.skip-set',
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

// Events that pass a manual gate or set a state by hand. Only a person may send them, never the
// agent token of a task terminal.
export const GATE_EVENT_TYPES: readonly EventType[] = [
  'review.approved',
  'stage.approved',
  'stage.passed',
  'stage.skipped',
  'state.overridden',
]

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
  'attachment.added',
  'stage.workflow-set',
  'stage.ci-set',
  'stage.ci-started',
  'stage.skip-set',
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
  // A prompt typed to an agent whose stage waits for approval takes the stage up again. So does a
  // prompt after an API error failed the turn: the agent's session goes on.
  'agent.resumed': { from: ['needs-input', 'review', 'failed'], to: 'running' },
  // The agent ended its turn without saying the stage is done (`natamux done`), for example to
  // ask something in plain text. The daemon drops the turn end that follows a report.
  'agent.finished': { from: ['running'], to: 'needs-input' },
  'agent.failed': { from: ['running', 'needs-input'], to: 'failed' },
  // The agent ended its turn, but its background work (a shell, a monitor, a scheduled prompt)
  // will wake it again, so the stage goes on running.
  'agent.waiting': { from: ['running'], to: 'running' },
  // Natamux ended the agent, for example with its workspace. The stage starts again by hand.
  'agent.stopped': { from: ['running', 'needs-input'], to: 'ready' },
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

// The position with the record of the paths taken, when there is one.
function withPaths(
  position: StagePosition,
  paths: Record<string, string> | undefined,
): StagePosition {
  return paths === undefined ? position : { ...position, paths }
}

// The state of one stage of a step: its lane in a group, else the state of the step.
function laneStateOf(position: StagePosition, step: Step, stage: string): TaskState | undefined {
  return isGroup(step) ? position.lanes[stage] : position.state
}

// Events that send a task back to a stage. A prompt typed to the agent during review is a
// request for changes.
function sendsBack(event: TaskEvent, step: Step): boolean {
  return (
    event.type === 'stage.reopened' ||
    event.type === 'review.changes-requested' ||
    (event.type === 'agent.resumed' && isStage(step) && step.kind === 'review')
  )
}

// The stage to go back to: `to`; for changes asked about an agent stage that waits for you, that
// stage; else the last agent stage before the current step. Only stages on the way the task went
// count: a stage of a path it did not take is never a target.
function reopenTarget(
  steps: readonly Step[],
  position: StagePosition,
  current: number,
  event: TaskEvent,
): { index: number; stage: string } | undefined {
  const to = payloadString(event, 'to')
  const here = pathOf(steps, position.stage)
  if (to !== undefined) {
    const index = stepIndex(steps, to)
    const there = pathOf(steps, to)
    const ahead = here !== undefined && there?.path === here.path && there.index > here.index
    return index >= 0 && index <= current && !ahead && !isNotTaken(steps, position.paths, to)
      ? { index, stage: to }
      : undefined
  }
  const step = stepToRun(steps, position.stage)
  if (event.type === 'review.changes-requested' && step !== undefined) {
    const waiting = stagesOf(step).filter((stage) => {
      const state = laneStateOf(position, step, stage.id)
      return stage.kind === 'agent' && (state === 'review' || state === 'needs-input')
    })
    const code = agentStageOf(step)
    const chosen = waiting.find((stage) => stage.id === code?.id) ?? waiting[0]
    if (chosen !== undefined) {
      return { index: current, stage: chosen.id }
    }
  }
  const earlier = [
    ...steps.slice(0, current).flatMap((each, index) => {
      const units: Step[] = isBranch(each) ? takenStages(each, position.paths) : [each]
      return units.map((unit) => ({ index, unit }))
    }),
    ...(here?.path.stages.slice(0, here.index).map((unit) => ({ index: current, unit })) ?? []),
  ]
  for (const { index, unit } of earlier.reverse()) {
    const stage = agentStageOf(unit)
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

// The position after going back to `stage` (a stage, or a whole group) at step `index`. The stage
// gets `state`, or without one the state it starts in, so a stage that only waits for a person
// waits for approval again. Going back inside the current group leaves its other stages as they
// are; going back to an earlier group counts its other stages as done. The choices made at that
// step and after it are dropped, so the task checks those conditions again; a stage of a path the
// task did not take is refused.
function reopen(
  steps: readonly Step[],
  index: number,
  stage: string,
  position: StagePosition,
  state?: TaskState,
): StagePosition | undefined {
  const step = steps[index]
  if (step === undefined) {
    return undefined
  }
  const stateOf = (target: Stage) => state ?? entryState(target)
  const paths =
    position.paths === undefined
      ? undefined
      : Object.fromEntries(
          Object.entries(position.paths).filter(([id]) => stepIndex(steps, id) < index),
        )
  if (isBranch(step)) {
    const place = pathOf(steps, stage)
    const target = place?.path.stages[place.index]
    if (place === undefined || target === undefined || isNotTaken(steps, position.paths, stage)) {
      return undefined
    }
    const taken = { ...paths, [step.id]: place.path.id }
    return { stage, state: stateOf(target), lanes: {}, paths: taken }
  }
  if (!isGroup(step)) {
    return withPaths({ stage: step.id, state: stateOf(step), lanes: {} }, paths)
  }
  const here = step.id === position.stage
  const lanes = Object.fromEntries(
    step.parallel.map((lane) => [
      lane.id,
      lane.id === stage || step.id === stage
        ? stateOf(lane)
        : here
          ? (position.lanes[lane.id] ?? 'done')
          : 'done',
    ]),
  ) as Record<string, TaskState>
  return withPaths({ stage: step.id, state: combinedState(lanes), lanes }, paths)
}

// The move an event makes in one stage. A failed check keeps the stage running while the agent
// fixes it; the last try waits for you.
function moveOf(event: TaskEvent): Move | undefined {
  return event.type === 'stage.check-failed'
    ? { from: ['running'], to: event.payload.final === true ? 'needs-input' : 'running' }
    : MOVES[event.type]
}

// The stage an event is about. Outside a group it is the current stage. Inside one: the stage it
// names, else the stage of the terminal it came from, else the only stage the move fits.
function laneOf(
  steps: readonly Step[],
  step: Step,
  position: StagePosition,
  event: TaskEvent,
  move: Move,
): string | undefined {
  const named = payloadString(event, 'stage')
  if (!isGroup(step) || named !== undefined) {
    return named ?? step.id
  }
  const terminal = payloadString(event, 'terminal')
  if (terminal !== undefined) {
    return stageOfTerminal(steps, step, terminal)?.id
  }
  const fitting = step.parallel.filter((stage) => {
    const state = position.lanes[stage.id]
    return state !== undefined && move.from.includes(state)
  })
  return fitting.length === 1 ? fitting[0]?.id : undefined
}

// The event as it is logged: the stage it is about, and for a send back the stage it goes back
// to, filled in when the sender left them out (a button, `natamux ask`, a terminal bell). The
// progress views read both from the log.
export function completeEvent(
  position: StagePosition,
  event: TaskEvent,
  steps: readonly Step[],
): TaskEvent {
  const index = stepIndex(steps, position.stage)
  const step = stepToRun(steps, position.stage)
  if (step === undefined || isTerminalState(position.state)) {
    return event
  }
  if (sendsBack(event, step)) {
    const target =
      payloadString(event, 'to') === undefined
        ? reopenTarget(steps, position, index, event)
        : undefined
    return target === undefined
      ? event
      : { ...event, payload: { ...event.payload, to: target.stage } }
  }
  const move = moveOf(event)
  if (move === undefined || payloadString(event, 'stage') !== undefined) {
    return event
  }
  const lane = laneOf(steps, step, position, event, move)
  return lane === undefined ? event : { ...event, payload: { ...event.payload, stage: lane } }
}

// After a stage ends: the next stage of its path, the next step, or done after the last one.
// `path` is the path the event took at a branch it enters.
function finishLane(
  steps: readonly Step[],
  index: number,
  position: StagePosition,
  lane: string,
  path: string | undefined,
): { position: StagePosition; moved: boolean } {
  const step = steps[index]
  if (step !== undefined && isGroup(step)) {
    const lanes = { ...position.lanes, [lane]: 'done' as TaskState }
    if (Object.values(lanes).some((state) => state !== 'done')) {
      return { position: { ...position, lanes, state: combinedState(lanes) }, moved: false }
    }
  }
  const place = pathOf(steps, lane)
  const following = place?.path.stages[place.index + 1]
  if (following !== undefined) {
    return { position: withPaths(enterStep(following), position.paths), moved: true }
  }
  let paths = position.paths
  let chosen = path
  for (const next of steps.slice(index + 1)) {
    if (!isBranch(next)) {
      return { position: withPaths(enterStep(next), paths), moved: true }
    }
    // A branch takes the path the event names. Without one, or with a path the task does not
    // have, the task skips the branch. Both are recorded, a skip as ''.
    const taken = next.paths.find((each) => each.id === chosen)?.id ?? ''
    paths = { ...paths, [next.id]: taken }
    if (taken !== '') {
      return { position: withPaths(enterStep(next, undefined, taken), paths), moved: true }
    }
    // The event chose a path for the first branch it reaches only.
    chosen = undefined
  }
  return { position: withPaths({ ...position, state: 'done', lanes: {} }, paths), moved: false }
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
  const step = stepToRun(steps, position.stage)

  if (event.type === 'state.overridden') {
    const target = taskStateSchema.safeParse(event.payload.to)
    if (!target.success) {
      return keep(false)
    }
    const stage = payloadString(event, 'stage')
    if (stage !== undefined) {
      const at = stepIndex(steps, stage)
      const moved = at >= 0 ? reopen(steps, at, stage, position, target.data) : undefined
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
    const reviewOrLater = (stage: Stage) => stage.kind === 'review' || stage.kind === 'release'
    const firstReview = steps.findIndex((candidate) =>
      takenStages(candidate, position.paths).some(reviewOrLater),
    )
    // The stages of a path share the index of their branch, so the stage must also be at or
    // after the review of its path.
    const place = pathOf(steps, position.stage)
    const early =
      index === firstReview &&
      place !== undefined &&
      !place.path.stages.slice(0, place.index + 1).some(reviewOrLater)
    if (firstReview < 0 || index < firstReview || early) {
      return keep(false)
    }
    // A branch is no stage to be at: at the end, the task ends on the last stage of its path.
    const last = steps.at(-1)
    const end =
      last !== undefined && isBranch(last) ? takenStages(last, position.paths ?? {}).at(-1) : last
    const done: StagePosition = { stage: end?.id ?? position.stage, state: 'done', lanes: {} }
    return {
      position: withPaths(done, position.paths),
      accepted: true,
      moved: end !== undefined && end.id !== position.stage,
    }
  }

  if (step === undefined) {
    return keep(false)
  }

  if (sendsBack(event, step)) {
    // The stage starts again, so the runner gives it its prompt and it gets a new round.
    const target = reopenTarget(steps, position, index, event)
    const moved =
      target === undefined ? undefined : reopen(steps, target.index, target.stage, position)
    return moved === undefined ? keep(false) : { position: moved, accepted: true, moved: true }
  }

  const move = moveOf(event)
  if (move === undefined) {
    return keep(false)
  }

  const group = isGroup(step)
  const lane = laneOf(steps, step, position, event, move)
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
    const finished = finishLane(steps, index, position, lane, payloadString(event, 'path'))
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

// The branch an event would enter: the event ends the current step, and the next step is a
// branch. The daemon checks the conditions of that branch before it applies the event. This is a
// dry run that ignores any path the event names.
export function entersBranch(
  position: StagePosition,
  event: TaskEvent,
  steps: readonly Step[],
): StageBranch | undefined {
  const before = position.paths ?? {}
  const payload = Object.fromEntries(
    Object.entries(event.payload).filter(([key]) => key !== 'path'),
  )
  const applied = applyTaskEvent({ ...position, paths: before }, { ...event, payload }, steps)
  const after = applied.position.paths ?? {}
  const entered = steps.find((step) => after[step.id] === '' && before[step.id] === undefined)
  return entered !== undefined && isBranch(entered) ? entered : undefined
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
