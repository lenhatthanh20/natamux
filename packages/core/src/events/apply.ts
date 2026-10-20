import { takenStages } from '../stages/capabilities.js'
import {
  combinedState,
  enterStep,
  pathOf,
  type StagePosition,
  stepIndex,
  stepToRun,
} from '../stages/position.js'
import { isBranch, isGroup, type Stage, type Step, stagesOf } from '../stages/schema.js'
import type { PinnedTerminals } from '../stages/terminal-lane.js'
import { isTerminalState, type TaskState, taskStateSchema } from '../tasks/task-state.js'
import { laneOf, moveOf, payloadString, withPaths } from './moves.js'
import { reopen, reopenTarget, sendsBack } from './reopen.js'
import { RECORD_ONLY_EVENT_TYPES, type TaskEvent } from './schema.js'

export type AppliedEvent = {
  position: StagePosition
  accepted: boolean
  // The step changed: the task entered a new stage or group, or went back to one.
  moved: boolean
}

// The event as it is logged: the stage it is about, and for a send back the stage it goes back
// to, filled in when the sender left them out (a button, `natamux ask`, a terminal bell). The
// progress views read both from the log. `pinned`: the task's pinned terminals, which name the stage
// of an event that names its terminal only, such as a terminal bell.
export function completeEvent(
  position: StagePosition,
  event: TaskEvent,
  steps: readonly Step[],
  pinned: PinnedTerminals = {},
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
  const lane = laneOf(steps, step, position, event, move, pinned)
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

const keep = (position: StagePosition, accepted: boolean): AppliedEvent => ({
  position,
  accepted,
  moved: false,
})

// A state set by hand: on a stage (it is reopened), on a group (every lane that is not done),
// or on the current step.
function overrideState(
  position: StagePosition,
  event: TaskEvent,
  steps: readonly Step[],
  step: Step | undefined,
): AppliedEvent {
  const target = taskStateSchema.safeParse(event.payload.to)
  if (!target.success) {
    return keep(position, false)
  }
  const stage = payloadString(event, 'stage')
  if (stage !== undefined) {
    const at = stepIndex(steps, stage)
    const moved = at >= 0 ? reopen(steps, at, stage, position, target.data) : undefined
    return moved === undefined
      ? keep(position, false)
      : { position: moved, accepted: true, moved: true }
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

function completeOnMerge(
  position: StagePosition,
  steps: readonly Step[],
  index: number,
): AppliedEvent {
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
    return keep(position, false)
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

function sendBack(
  position: StagePosition,
  event: TaskEvent,
  steps: readonly Step[],
  index: number,
): AppliedEvent {
  // The stage starts again, so the runner gives it its prompt and it gets a new round.
  const target = reopenTarget(steps, position, index, event)
  const moved =
    target === undefined ? undefined : reopen(steps, target.index, target.stage, position)
  return moved === undefined
    ? keep(position, false)
    : { position: moved, accepted: true, moved: true }
}

// A plain move of a stage or a lane: start, ask, end. An end goes to the gate or on to the next
// stage, by the gate of the stage.
function moveStage(
  position: StagePosition,
  event: TaskEvent,
  steps: readonly Step[],
  index: number,
  step: Step,
  move: NonNullable<ReturnType<typeof moveOf>>,
): AppliedEvent {
  const group = isGroup(step)
  const lane = laneOf(steps, step, position, event, move)
  const stage = stagesOf(step).find((candidate) => candidate.id === lane)
  if (lane === undefined || stage === undefined) {
    return keep(position, false)
  }
  const current = group ? position.lanes[lane] : position.state
  if (current === undefined || !move.from.includes(current)) {
    return keep(position, false)
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

// Applies one event to a task's position in its steps. An event that does not fit is not
// accepted and leaves the position as it is.
export function applyTaskEvent(
  position: StagePosition,
  event: TaskEvent,
  steps: readonly Step[],
): AppliedEvent {
  if (RECORD_ONLY_EVENT_TYPES.includes(event.type)) {
    return keep(position, true)
  }

  const index = stepIndex(steps, position.stage)
  const step = stepToRun(steps, position.stage)

  if (event.type === 'state.overridden') {
    return overrideState(position, event, steps, step)
  }

  if (isTerminalState(position.state)) {
    return keep(position, false)
  }

  if (event.type === 'task.cancelled') {
    return { position: { ...position, state: 'cancelled' }, accepted: true, moved: false }
  }

  if (event.type === 'pr.merged') {
    return completeOnMerge(position, steps, index)
  }

  if (step === undefined) {
    return keep(position, false)
  }

  if (sendsBack(event, step)) {
    return sendBack(position, event, steps, index)
  }

  const move = moveOf(event)
  return move === undefined
    ? keep(position, false)
    : moveStage(position, event, steps, index, step, move)
}
