import {
  isBranch,
  isGroup,
  isStage,
  type Stage,
  type StageBranch,
  type StageGroup,
  type Step,
  stagesOf,
} from '@natamux/core'
import { MAX_ID } from './settings-json'

// Every id in the pipeline: stages, groups, branches and paths.
export function usedIds(steps: readonly Step[]): Set<string> {
  return new Set(
    steps.flatMap((step) => {
      if (isGroup(step)) {
        return [step.id, ...step.parallel.map((stage) => stage.id)]
      }
      if (isBranch(step)) {
        return [
          step.id,
          ...step.paths.flatMap((path) => [path.id, ...path.stages.map((stage) => stage.id)]),
        ]
      }
      return [step.id]
    }),
  )
}

// Where a stage is: its step; its lane when it runs in a group; its path and its place in the path
// when it is in a branch. A group's or a branch's own id has none of these.
type Place = { index: number; lane: number | null; path: number | null; slot: number | null }

export function findPlace(steps: readonly Step[], id: string): Place | null {
  for (const [index, step] of steps.entries()) {
    if (step.id === id) {
      return { index, lane: null, path: null, slot: null }
    }
    if (isGroup(step)) {
      const lane = step.parallel.findIndex((stage) => stage.id === id)
      if (lane !== -1) {
        return { index, lane, path: null, slot: null }
      }
    }
    if (isBranch(step)) {
      for (const [path, item] of step.paths.entries()) {
        const slot = item.stages.findIndex((stage) => stage.id === id)
        if (slot !== -1) {
          return { index, lane: null, path, slot }
        }
      }
    }
  }
  return null
}

export function findBranch(steps: readonly Step[], id: string): StageBranch | undefined {
  const step = steps.find((candidate) => candidate.id === id)
  return step !== undefined && isBranch(step) ? step : undefined
}

// The branch with this path, and the place of the path in it.
export function findPath(
  steps: readonly Step[],
  pathId: string,
): { index: number; path: number } | null {
  for (const [index, step] of steps.entries()) {
    if (isBranch(step)) {
      const path = step.paths.findIndex((item) => item.id === pathId)
      if (path !== -1) {
        return { index, path }
      }
    }
  }
  return null
}

// A path holds single stages; a context or release stage has its fixed place and cannot be in one.
export function fitsInPath(step: Step): step is Stage {
  return isStage(step) && step.kind !== 'context' && step.kind !== 'release'
}

export function findStage(steps: readonly Step[], id: string): Stage | undefined {
  return steps.flatMap(stagesOf).find((stage) => stage.id === id)
}

// The group a stage runs in, if any.
export function groupOf(steps: readonly Step[], id: string): StageGroup | undefined {
  const place = findPlace(steps, id)
  const step = place === null || place.lane === null ? undefined : steps[place.index]
  return step !== undefined && isGroup(step) ? step : undefined
}

export function replaceAt(steps: readonly Step[], index: number, ...replacement: Step[]): Step[] {
  return [...steps.slice(0, index), ...replacement, ...steps.slice(index + 1)]
}

export function insertAt(steps: readonly Step[], index: number, step: Step): Step[] {
  return [...steps.slice(0, index), step, ...steps.slice(index)]
}

// The base with a number after it, when the base is used already.
export function freeId(used: ReadonlySet<string>, base: string): string {
  if (!used.has(base)) {
    return base
  }
  let number = 2
  while (used.has(`${base}-${number}`)) {
    number += 1
  }
  return `${base}-${number}`
}

// "path-3", counting up from `from` until no id of the pipeline is the same.
export function freePathId(used: ReadonlySet<string>, from: number): string {
  let number = from
  while (used.has(`path-${number}`)) {
    number += 1
  }
  return `path-${number}`
}

// A group id that no stage or group uses yet, from the id of its first stage.
export function newGroupId(steps: readonly Step[], first: string): string {
  return freeId(usedIds(steps), first.length <= MAX_ID - 10 ? `${first}-group` : 'group')
}

export function releaseIndex(steps: readonly Step[]): number {
  const release = steps.findIndex((step) => isStage(step) && step.kind === 'release')
  return release === -1 ? steps.length : release
}

export function stepOfStage(steps: readonly Step[], id: string): Step | undefined {
  return steps.find((candidate) => stagesOf(candidate).some((stage) => stage.id === id))
}
