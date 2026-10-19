import { takenStages } from '../stages/capabilities.js'
import {
  combinedState,
  entryState,
  isNotTaken,
  pathOf,
  type StagePosition,
  stepIndex,
  stepToRun,
} from '../stages/position.js'
import { isBranch, isGroup, isStage, type Stage, type Step, stagesOf } from '../stages/schema.js'
import type { TaskState } from '../tasks/task-state.js'
import { laneStateOf, payloadString, withPaths } from './moves.js'
import type { TaskEvent } from './schema.js'

// Events that send a task back to a stage. A prompt typed to the agent during review is a
// request for changes.
export function sendsBack(event: TaskEvent, step: Step): boolean {
  return (
    event.type === 'stage.reopened' ||
    event.type === 'review.changes-requested' ||
    (event.type === 'agent.resumed' && isStage(step) && step.kind === 'review')
  )
}

// The stage to go back to: `to`; for changes asked about an agent stage that waits for you, that
// stage; else the last agent stage before the current step. Only stages on the way the task went
// count: a stage of a path it did not take is never a target.
export function reopenTarget(
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
export function reopen(
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
