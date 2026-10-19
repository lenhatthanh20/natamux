import {
  GROUP_KINDS,
  isBranch,
  isGroup,
  isStage,
  type Stage,
  type StageBranch,
  type StageGroup,
  type Step,
  stageName,
} from '@natamux/core'
import { findPath, findPlace, fitsInPath, insertAt, newGroupId, replaceAt } from './find'
import { branchName } from './labels'
import { MAX_GROUP } from './settings-json'

function withPathStages(branch: StageBranch, path: number, stages: Stage[]): StageBranch {
  return {
    ...branch,
    paths: branch.paths.map((item, at) => (at === path ? { ...item, stages } : item)),
  }
}

// A group of one stage is no group: the stage runs on its own.
function regroup(group: StageGroup, parallel: Stage[]): Step {
  const [only] = parallel
  return parallel.length === 1 && only !== undefined ? only : { ...group, parallel }
}

// Takes a step or a stage out. `at` is where the rest has the step it came from.
function takeOut(
  steps: readonly Step[],
  id: string,
): { rest: Step[]; taken: Step; at: number } | null {
  const place = findPlace(steps, id)
  const step = place === null ? undefined : steps[place.index]
  if (place === null || step === undefined) {
    return null
  }
  if (place.path !== null && place.slot !== null && isBranch(step)) {
    const stages = step.paths[place.path]?.stages ?? []
    const taken = stages[place.slot] as Stage
    const left = stages.filter((_, slot) => slot !== place.slot)
    const rest = replaceAt(steps, place.index, withPathStages(step, place.path, left))
    return { rest, taken, at: place.index }
  }
  if (place.lane === null || !isGroup(step)) {
    return { rest: replaceAt(steps, place.index), taken: step, at: place.index }
  }
  const taken = step.parallel[place.lane] as Stage
  const left = step.parallel.filter((_, lane) => lane !== place.lane)
  return { rest: replaceAt(steps, place.index, regroup(step, left)), taken, at: place.index }
}

// One place up (-1) or down (1). A stage in a group moves among its lanes, and leaves the group
// at its edge. Gives the same array back when nothing can move.
export function moveStage(steps: readonly Step[], id: string, direction: -1 | 1): Step[] {
  const place = findPlace(steps, id)
  const step = place === null ? undefined : steps[place.index]
  if (place === null || step === undefined) {
    return steps as Step[]
  }
  if (place.path !== null && place.slot !== null && isBranch(step)) {
    return movePathStage(steps, id, step, place.path, place.slot, direction)
  }
  if (place.lane !== null && isGroup(step)) {
    const target = place.lane + direction
    if (target >= 0 && target < step.parallel.length) {
      const parallel = [...step.parallel]
      ;[parallel[place.lane], parallel[target]] = [
        parallel[target] as Stage,
        parallel[place.lane] as Stage,
      ]
      return replaceAt(steps, place.index, { ...step, parallel })
    }
    const out = takeOut(steps, id)
    if (out === null) {
      return steps as Step[]
    }
    return insertAt(out.rest, direction < 0 ? out.at : out.at + 1, out.taken)
  }
  const target = place.index + direction
  const other = steps[target]
  if (other === undefined) {
    return steps as Step[]
  }
  // A stage next to a branch goes into its nearest path; it leaves by the same controls.
  if (isBranch(other) && fitsInPath(step)) {
    const out = takeOut(steps, id)
    const at = out === null ? -1 : out.rest.findIndex((item) => item.id === other.id)
    const branch = out?.rest[at]
    if (out === null || branch === undefined || !isBranch(branch)) {
      return steps as Step[]
    }
    const path = direction > 0 ? 0 : branch.paths.length - 1
    const held = branch.paths[path]?.stages ?? []
    const stages = direction > 0 ? [step, ...held] : [...held, step]
    return replaceAt(out.rest, at, withPathStages(branch, path, stages))
  }
  return replaceAt(replaceAt(steps, target, step), place.index, other)
}

// A stage of a path moves among the stages of its path, then into the next path, and out of the
// branch at the last one.
function movePathStage(
  steps: readonly Step[],
  id: string,
  branch: StageBranch,
  path: number,
  slot: number,
  direction: -1 | 1,
): Step[] {
  const stages = [...(branch.paths[path]?.stages ?? [])]
  const target = slot + direction
  if (target >= 0 && target < stages.length) {
    ;[stages[slot], stages[target]] = [stages[target] as Stage, stages[slot] as Stage]
    return replaceAt(steps, steps.indexOf(branch), withPathStages(branch, path, stages))
  }
  const out = takeOut(steps, id)
  if (out === null) {
    return steps as Step[]
  }
  const stage = out.taken as Stage
  const next = path + direction
  const left = out.rest[out.at]
  if (left !== undefined && isBranch(left) && next >= 0 && next < left.paths.length) {
    const held = left.paths[next]?.stages ?? []
    const joined = direction < 0 ? [...held, stage] : [stage, ...held]
    return replaceAt(out.rest, out.at, withPathStages(left, next, joined))
  }
  return insertAt(out.rest, direction < 0 ? out.at : out.at + 1, stage)
}

export function canMove(steps: readonly Step[], id: string, direction: -1 | 1): boolean {
  return moveStage(steps, id, direction) !== steps
}

// Drag and drop: puts a stage or a group before or after another. Dropped on a stage of a group,
// a stage that may run side by side joins that group when it has room.
export function dropStage(
  steps: readonly Step[],
  id: string,
  targetId: string,
  side: 'before' | 'after',
): Step[] {
  if (id === targetId) {
    return steps as Step[]
  }
  const from = findPlace(steps, id)
  // Dropped on a path that has no stage to drop beside: the stage goes to the end of the path.
  const into = findPath(steps, targetId)
  if (from !== null && into !== null) {
    return dropIntoPath(steps, id, into)
  }
  const to = findPlace(steps, targetId)
  if (from === null || to === null) {
    return steps as Step[]
  }
  // Within one group: only the lanes change.
  const group = steps[from.index]
  if (
    from.lane !== null &&
    to.lane !== null &&
    from.index === to.index &&
    group !== undefined &&
    isGroup(group)
  ) {
    const moving = group.parallel[from.lane] as Stage
    const left = group.parallel.filter((_, lane) => lane !== from.lane)
    const target = left.findIndex((stage) => stage.id === targetId)
    const parallel = [...left]
    parallel.splice(side === 'before' ? target : target + 1, 0, moving)
    return replaceAt(steps, from.index, { ...group, parallel })
  }
  const out = takeOut(steps, id)
  if (out === null) {
    return steps as Step[]
  }
  const place = findPlace(out.rest, targetId)
  const host = place === null ? undefined : out.rest[place.index]
  if (place === null || host === undefined) {
    return steps as Step[]
  }
  const { taken } = out
  if (place.path !== null && place.slot !== null && isBranch(host)) {
    if (!fitsInPath(taken)) {
      return steps as Step[]
    }
    const stages = [...(host.paths[place.path]?.stages ?? [])]
    stages.splice(side === 'before' ? place.slot : place.slot + 1, 0, taken)
    return replaceAt(out.rest, place.index, withPathStages(host, place.path, stages))
  }
  if (
    place.lane !== null &&
    isGroup(host) &&
    isStage(taken) &&
    GROUP_KINDS.includes(taken.kind) &&
    host.parallel.length < MAX_GROUP
  ) {
    const parallel = [...host.parallel]
    parallel.splice(side === 'before' ? place.lane : place.lane + 1, 0, taken)
    return replaceAt(out.rest, place.index, { ...host, parallel })
  }
  return insertAt(out.rest, side === 'before' ? place.index : place.index + 1, taken)
}

function dropIntoPath(
  steps: readonly Step[],
  id: string,
  into: { index: number; path: number },
): Step[] {
  const target = steps[into.index]
  const out = takeOut(steps, id)
  if (out === null || target === undefined || !fitsInPath(out.taken)) {
    return steps as Step[]
  }
  const at = out.rest.findIndex((step) => step.id === target.id)
  const branch = out.rest[at]
  if (branch === undefined || !isBranch(branch)) {
    return steps as Step[]
  }
  const held = branch.paths[into.path]?.stages ?? []
  return replaceAt(out.rest, at, withPathStages(branch, into.path, [...held, out.taken]))
}

// Why a stage cannot run alongside the stage above, or null when it can.
export function joinProblem(steps: readonly Step[], id: string): string | null {
  const place = findPlace(steps, id)
  const step = place === null ? undefined : steps[place.index]
  if (place !== null && place.lane !== null) {
    return 'It runs alongside other stages already'
  }
  if (place !== null && place.path !== null) {
    return 'A path holds single stages only'
  }
  if (place === null || step === undefined || !isStage(step)) {
    return 'Only a stage can run alongside another'
  }
  if (!GROUP_KINDS.includes(step.kind)) {
    return 'Only agent and manual stages run side by side'
  }
  const above = steps[place.index - 1]
  if (above === undefined) {
    return 'There is no stage above'
  }
  if (isBranch(above)) {
    return `${branchName(above)} is a branch; a stage cannot run alongside it`
  }
  if (isGroup(above)) {
    return above.parallel.length >= MAX_GROUP ? 'A group holds 4 stages at most' : null
  }
  return GROUP_KINDS.includes(above.kind)
    ? null
    : `${stageName(above)} cannot run side by side with other stages`
}

// Makes a stage run alongside the stage above: it joins the group above, or makes a new group of
// the two.
export function joinAbove(steps: readonly Step[], id: string): Step[] {
  const place = findPlace(steps, id)
  if (joinProblem(steps, id) !== null || place === null) {
    return steps as Step[]
  }
  const stage = steps[place.index] as Stage
  const above = steps[place.index - 1] as Step
  if (isBranch(above)) {
    return steps as Step[]
  }
  const rest = replaceAt(steps, place.index)
  const joined: StageGroup = isGroup(above)
    ? { ...above, parallel: [...above.parallel, stage] }
    : { id: newGroupId(steps, above.id), parallel: [above, stage] }
  return replaceAt(rest, place.index - 1, joined)
}

// Takes a stage out of its group; it runs right after the group.
export function runAlone(steps: readonly Step[], id: string): Step[] {
  const place = findPlace(steps, id)
  if (place === null || place.lane === null) {
    return steps as Step[]
  }
  const out = takeOut(steps, id)
  return out === null ? (steps as Step[]) : insertAt(out.rest, out.at + 1, out.taken)
}

export function removeStage(steps: readonly Step[], id: string): Step[] {
  return takeOut(steps, id)?.rest ?? (steps as Step[])
}
