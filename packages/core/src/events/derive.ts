import { firstPosition, type StagePosition } from '../stages/position.js'
import { isBranch, type StageBranch, type Step } from '../stages/schema.js'
import { applyTaskEvent } from './apply.js'
import type { TaskEvent } from './schema.js'

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
