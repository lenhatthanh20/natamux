import {
  type BranchPath,
  type Condition,
  conditionSchema,
  isBranch,
  type StageBranch,
  type Step,
} from '@natamux/core'
import { findBranch, findPlace, freeId, freePathId, insertAt, releaseIndex, usedIds } from './find'
import { MAX_PATHS } from './settings-json'

function updateBranch(
  steps: readonly Step[],
  branchId: string,
  change: (branch: StageBranch) => StageBranch,
): Step[] {
  return steps.map((step) => (isBranch(step) && step.id === branchId ? change(step) : step))
}

export function updatePath(
  steps: readonly Step[],
  pathId: string,
  change: (path: BranchPath) => BranchPath,
): Step[] {
  return steps.map((step) =>
    isBranch(step) && step.paths.some((path) => path.id === pathId)
      ? { ...step, paths: step.paths.map((path) => (path.id === pathId ? change(path) : path)) }
      : step,
  )
}

// A condition to start from. It is not valid until the person fills it in.
export function blankCondition(type: Condition['type']): Condition {
  return type === 'command'
    ? { type: 'command', run: '' }
    : { type: 'output', file: 'plan.md', contains: '' }
}

// The condition with another type; the same one when the type does not change.
export function retypeCondition(when: Condition | undefined, type: Condition['type']): Condition {
  return when?.type === type ? when : blankCondition(type)
}

export type ConditionField = 'file' | 'section' | 'contains' | 'run'

// One field of a condition. An empty section is left out.
export function editCondition(when: Condition, field: ConditionField, value: string): Condition {
  if (when.type === 'command') {
    return field === 'run' ? { ...when, run: value } : when
  }
  if (field === 'file') {
    return { ...when, file: value }
  }
  if (field === 'contains') {
    return { ...when, contains: value }
  }
  if (field === 'section') {
    const { section: _, ...rest } = when
    return value.trim() === '' ? rest : { ...rest, section: value }
  }
  return when
}

// The messages of the rules a condition breaks, by field.
export function conditionProblems(when: Condition): Partial<Record<ConditionField, string>> {
  const problems: Partial<Record<ConditionField, string>> = {}
  for (const issue of conditionSchema.safeParse(when).error?.issues ?? []) {
    const field = issue.path[0]
    if (field === 'file' || field === 'section' || field === 'contains' || field === 'run') {
      problems[field] ??= issue.message
    }
  }
  return problems
}

// A new branch goes after the step with this id, or before the release. It starts with a path that
// has a condition and an Otherwise path, both without stages: the person adds them.
export function addBranch(steps: readonly Step[], afterId?: string): Step[] {
  const place = afterId === undefined ? null : findPlace(steps, afterId)
  const end = releaseIndex(steps)
  const index = place === null ? end : Math.min(place.index + 1, end)
  const used = usedIds(steps)
  const id = freeId(used, 'branch')
  used.add(id)
  const first = freePathId(used, 1)
  used.add(first)
  const second = freePathId(used, 2)
  const branch: StageBranch = {
    id,
    paths: [
      { id: first, when: blankCondition('output'), stages: [] },
      { id: second, stages: [] },
    ],
  }
  return insertAt(steps, index, branch)
}

// A path with a condition. It goes before the Otherwise path, which stays last. A branch holds 4
// paths at most.
export function addPath(steps: readonly Step[], branchId: string): Step[] {
  const branch = findBranch(steps, branchId)
  if (branch === undefined || branch.paths.length >= MAX_PATHS) {
    return steps as Step[]
  }
  const id = freePathId(usedIds(steps), branch.paths.length + 1)
  const added: BranchPath = { id, when: blankCondition('output'), stages: [] }
  const last = branch.paths.at(-1)
  return updateBranch(steps, branchId, (item) => ({
    ...item,
    paths:
      last !== undefined && last.when === undefined
        ? [...item.paths.slice(0, -1), added, last]
        : [...item.paths, added],
  }))
}

// The path goes with its stages. A branch keeps one path at least.
export function removePath(steps: readonly Step[], branchId: string, pathId: string): Step[] {
  const branch = findBranch(steps, branchId)
  if (branch === undefined || branch.paths.length < 2) {
    return steps as Step[]
  }
  return updateBranch(steps, branchId, (item) => ({
    ...item,
    paths: item.paths.filter((path) => path.id !== pathId),
  }))
}

// "Does not hold" is `not: true`; "holds" leaves the field out.
export function setPathCondition(
  steps: readonly Step[],
  branchId: string,
  pathId: string,
  when: Condition,
  not = false,
): Step[] {
  if (findBranch(steps, branchId) === undefined) {
    return steps as Step[]
  }
  return updatePath(steps, pathId, (path) => {
    const { not: _, ...rest } = path
    return { ...rest, when, ...(not ? { not: true } : {}) }
  })
}

// Only the last path can be the Otherwise path. Turning it off gives it a blank condition.
export function setOtherwise(
  steps: readonly Step[],
  branchId: string,
  pathId: string,
  on: boolean,
): Step[] {
  const last = findBranch(steps, branchId)?.paths.at(-1)
  if (last === undefined || last.id !== pathId || (last.when === undefined) === on) {
    return steps as Step[]
  }
  return updatePath(steps, pathId, (path) => {
    const { when: _when, not: _not, ...rest } = path
    return on ? rest : { ...rest, when: blankCondition('output') }
  })
}

// An empty name removes the field, so the id shows as the name.
export function setBranchName(steps: readonly Step[], branchId: string, name: string): Step[] {
  return updateBranch(steps, branchId, (branch) => {
    const { name: _, ...rest } = branch
    return name.trim() === '' ? rest : { ...rest, name }
  })
}

export function setPathName(
  steps: readonly Step[],
  branchId: string,
  pathId: string,
  name: string,
): Step[] {
  if (findBranch(steps, branchId) === undefined) {
    return steps as Step[]
  }
  return updatePath(steps, pathId, (path) => {
    const { name: _, ...rest } = path
    return name.trim() === '' ? rest : { ...rest, name }
  })
}
