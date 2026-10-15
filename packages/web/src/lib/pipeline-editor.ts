import {
  activeSteps,
  type BranchPath,
  type CatalogStage,
  type Condition,
  type CreateIssueWhen,
  canHaveOwnAgent,
  catalogEntry,
  commitRule,
  conditionSchema,
  DEFAULT_PIPELINE,
  DEFAULT_PROJECT_SETTINGS,
  defaultPrompt,
  describeCondition,
  GATES,
  type Gate,
  GROUP_KINDS,
  isBranch,
  isGroup,
  isStage,
  type PipelineIssue,
  projectSettingsSchema,
  STAGE_CATALOG,
  STAGE_KINDS,
  type Stage,
  type StageBranch,
  type StageGroup,
  type StageKind,
  type Step,
  stageIdSchema,
  stageName,
  stageSchema,
  stagesOf,
  terminalOfStage,
  trackerSettingsSchema,
} from '@natamux/core'

// The logic of the Stages tab of Project settings. The draft is the text of `.natamux.json`; the
// tab reads `stages` from it and writes them back, and keeps every other key as it is.

export type RawSettings = Record<string, unknown>

export type Draft = { ok: true; raw: RawSettings; steps: Step[] } | { ok: false; error: string }

const ID_RULE = /^[a-z0-9]+(-[a-z0-9]+)*$/
const MAX_ID = 40
const MAX_GROUP = 4
const MAX_PATHS = 4

function isRecord(value: unknown): value is RawSettings {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

export function formatSettings(raw: unknown): string {
  return JSON.stringify(raw, null, 2)
}

// The same value with the keys of each object sorted, so two values compare as text.
function canonical(value: unknown): unknown {
  if (Array.isArray(value)) {
    return value.map(canonical)
  }
  if (isRecord(value)) {
    return Object.fromEntries(
      Object.keys(value)
        .sort()
        .filter((key) => value[key] !== undefined)
        .map((key) => [key, canonical(value[key])]),
    )
  }
  return value
}

export function sameJson(a: unknown, b: unknown): boolean {
  return JSON.stringify(canonical(a)) === JSON.stringify(canonical(b))
}

function tryParse(text: string): unknown {
  try {
    return JSON.parse(text)
  } catch {
    return undefined
  }
}

// Two drafts are the same when they hold the same settings, however they are formatted.
export function sameSettings(a: string, b: string): boolean {
  const left = tryParse(a)
  const right = tryParse(b)
  if (left === undefined || right === undefined) {
    return a === b
  }
  return sameJson(left, right)
}

function isStageShape(value: unknown): boolean {
  return (
    isRecord(value) &&
    typeof value.id === 'string' &&
    STAGE_KINDS.some((kind) => kind === value.kind) &&
    GATES.some((gate) => gate === value.gate)
  )
}

function isPathShape(value: unknown): boolean {
  return (
    isRecord(value) &&
    typeof value.id === 'string' &&
    Array.isArray(value.stages) &&
    value.stages.every(isStageShape)
  )
}

// The steps the Stages tab can show. Wrong values of a field are left for `pipelineIssues`, which
// names them next to their stage; only a shape the list cannot draw is refused here.
function readSteps(value: unknown): Step[] | string {
  if (value === undefined) {
    return DEFAULT_PIPELINE
  }
  if (!Array.isArray(value)) {
    return 'stages: give a list of stages, such as [].'
  }
  for (const [index, step] of value.entries()) {
    if (isRecord(step) && 'paths' in step) {
      const fitsBranch =
        typeof step.id === 'string' && Array.isArray(step.paths) && step.paths.every(isPathShape)
      if (!fitsBranch) {
        return `stages, item ${index + 1}: a branch needs an id and paths, each with an id and a list of stages. A stage needs an id, a kind (${STAGE_KINDS.join(', ')}) and a gate (manual or auto).`
      }
      continue
    }
    const fits =
      isRecord(step) && 'parallel' in step
        ? typeof step.id === 'string' &&
          Array.isArray(step.parallel) &&
          step.parallel.every(isStageShape)
        : isStageShape(step)
    if (!fits) {
      return `stages, item ${index + 1}: each stage needs an id, a kind (${STAGE_KINDS.join(', ')}) and a gate (manual or auto).`
    }
  }
  return value as Step[]
}

const otherSettingsSchema = projectSettingsSchema.omit({ stages: true })

// Reads the draft text. Not JSON, not an object, or settings outside `stages` that break the
// rules: the person fixes it in the JSON tab.
export function parseDraft(text: string): Draft {
  let raw: unknown
  try {
    raw = JSON.parse(text)
  } catch (error) {
    return {
      ok: false,
      error: `This is not JSON: ${error instanceof Error ? error.message : String(error)}`,
    }
  }
  if (!isRecord(raw)) {
    return { ok: false, error: 'The settings are one JSON object, such as {}.' }
  }
  const steps = readSteps(raw.stages)
  if (typeof steps === 'string') {
    return { ok: false, error: steps }
  }
  const others = otherSettingsSchema.safeParse(raw)
  if (!others.success) {
    const problems = others.error.issues.map(
      (issue) => `${issue.path.join('.') || 'file'}: ${issue.message}`,
    )
    return { ok: false, error: problems.join('; ') }
  }
  return { ok: true, raw, steps }
}

// The settings with these steps. A pipeline equal to the default is left out, so the project
// follows later changes of the default. `stages` keeps its place among the keys.
export function withSteps(raw: RawSettings, steps: readonly Step[]): RawSettings {
  if (sameJson(steps, DEFAULT_PIPELINE)) {
    return Object.fromEntries(Object.entries(raw).filter(([key]) => key !== 'stages'))
  }
  if ('stages' in raw) {
    return Object.fromEntries(
      Object.entries(raw).map(([key, value]) => [key, key === 'stages' ? steps : value]),
    )
  }
  return { stages: steps, ...raw }
}

// The tracker of the draft, with the defaults for what it leaves out.
export function trackerOf(raw: RawSettings) {
  const parsed = trackerSettingsSchema.safeParse(raw.tracker ?? {})
  return parsed.success ? parsed.data : DEFAULT_PROJECT_SETTINGS.tracker
}

// The settings with this `tracker.createIssue`. The default is left out, like the default
// pipeline, and so is a tracker with nothing left. `tracker` keeps its place among the keys.
export function withCreateIssue(raw: RawSettings, createIssue: CreateIssueWhen): RawSettings {
  const old = isRecord(raw.tracker) ? raw.tracker : {}
  const rest = Object.fromEntries(Object.entries(old).filter(([key]) => key !== 'createIssue'))
  const tracker =
    createIssue === DEFAULT_PROJECT_SETTINGS.tracker.createIssue ? rest : { ...rest, createIssue }
  if (Object.keys(tracker).length === 0) {
    return Object.fromEntries(Object.entries(raw).filter(([key]) => key !== 'tracker'))
  }
  return { ...raw, tracker }
}

// The settings to save over a file that changed on disk: the keys the person changed since
// `start` (the file as it was loaded) take the draft's value; every other key keeps the value of
// the file now.
export function mergeChanges(
  start: RawSettings,
  draft: RawSettings,
  now: RawSettings,
): RawSettings {
  const changed = new Set(
    [...Object.keys(start), ...Object.keys(draft)].filter(
      (key) => !sameJson(start[key], draft[key]),
    ),
  )
  const merged: RawSettings = Object.fromEntries(
    Object.entries(now)
      .filter(([key]) => !changed.has(key) || key in draft)
      .map(([key, value]) => [key, changed.has(key) ? draft[key] : value]),
  )
  for (const key of changed) {
    if (key in draft && !(key in merged)) {
      merged[key] = draft[key]
    }
  }
  return merged
}

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
function findPath(steps: readonly Step[], pathId: string): { index: number; path: number } | null {
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
function fitsInPath(step: Step): step is Stage {
  return isStage(step) && step.kind !== 'context' && step.kind !== 'release'
}

function withPathStages(branch: StageBranch, path: number, stages: Stage[]): StageBranch {
  return {
    ...branch,
    paths: branch.paths.map((item, at) => (at === path ? { ...item, stages } : item)),
  }
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

function replaceAt(steps: readonly Step[], index: number, ...replacement: Step[]): Step[] {
  return [...steps.slice(0, index), ...replacement, ...steps.slice(index + 1)]
}

function insertAt(steps: readonly Step[], index: number, step: Step): Step[] {
  return [...steps.slice(0, index), step, ...steps.slice(index)]
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

// The base with a number after it, when the base is used already.
function freeId(used: ReadonlySet<string>, base: string): string {
  if (!used.has(base)) {
    return base
  }
  let number = 2
  while (used.has(`${base}-${number}`)) {
    number += 1
  }
  return `${base}-${number}`
}

// Two paths of a branch can break the same rule, and the message does not tell them apart: show it
// once.
export function uniqueIssues(issues: readonly PipelineIssue[]): PipelineIssue[] {
  const seen = new Set<string>()
  return issues.filter((issue) => {
    const key = `${issue.level}|${issue.stage}|${issue.message}`
    const fresh = !seen.has(key)
    seen.add(key)
    return fresh
  })
}

// "path-3", counting up from `from` until no id of the pipeline is the same.
function freePathId(used: ReadonlySet<string>, from: number): string {
  let number = from
  while (used.has(`path-${number}`)) {
    number += 1
  }
  return `path-${number}`
}

// A group id that no stage or group uses yet, from the id of its first stage.
function newGroupId(steps: readonly Step[], first: string): string {
  return freeId(usedIds(steps), first.length <= MAX_ID - 10 ? `${first}-group` : 'group')
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

// New stages go before the release, which must stay last. A context stage must come first.
function addBeforeRelease(steps: readonly Step[], stage: Stage): Step[] {
  if (stage.kind === 'context') {
    return insertAt(steps, 0, stage)
  }
  return insertAt(steps, releaseIndex(steps), stage)
}

function releaseIndex(steps: readonly Step[]): number {
  const release = steps.findIndex((step) => isStage(step) && step.kind === 'release')
  return release === -1 ? steps.length : release
}

// With `pathId`, the stage goes to the end of that path instead. A path takes no context or
// release stage.
export function addCatalogStage(steps: readonly Step[], id: string, pathId?: string): Step[] {
  const entry = catalogEntry(id)
  if (entry === undefined || usedIds(steps).has(id)) {
    return steps as Step[]
  }
  const { sections, ...rest } = entry.stage
  const stage: Stage = sections === undefined ? rest : { ...rest, sections: [...sections] }
  if (pathId === undefined) {
    return addBeforeRelease(steps, stage)
  }
  if (!fitsInPath(stage) || findPath(steps, pathId) === null) {
    return steps as Step[]
  }
  return updatePath(steps, pathId, (path) => ({ ...path, stages: [...path.stages, stage] }))
}

// The catalog stages that are not in the pipeline, in catalog order.
export function catalogChoices(steps: readonly Step[]): CatalogStage[] {
  const used = usedIds(steps)
  return STAGE_CATALOG.filter((entry) => !used.has(entry.stage.id))
}

export type CustomStageInput = {
  name: string
  id: string
  kind: CustomKind
  output: string
}

export const CUSTOM_KINDS = ['agent', 'testing', 'manual'] as const satisfies readonly StageKind[]
export type CustomKind = (typeof CUSTOM_KINDS)[number]

// An id from a name: "Load test" becomes "load-test".
export function idFromName(name: string): string {
  return name
    .toLowerCase()
    .normalize('NFKD')
    .replace(/[\u0300-\u036f]/g, '')
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .slice(0, MAX_ID)
    .replace(/-+$/, '')
}

export type CustomStageProblems = Partial<Record<'name' | 'id' | 'output', string>>

export function customStageProblems(
  steps: readonly Step[],
  input: CustomStageInput,
): CustomStageProblems {
  const problems: CustomStageProblems = {}
  const name = input.name.trim()
  if (name === '') {
    problems.name = 'Give the stage a name'
  } else if (name.length > 60) {
    problems.name = 'A name has 60 characters at most'
  }
  const id = input.id
  const catalog = catalogEntry(id)
  if (id === '') {
    problems.id = 'Give the stage an id'
  } else if (id.length > MAX_ID) {
    problems.id = `An id has ${MAX_ID} characters at most`
  } else if (!ID_RULE.test(id)) {
    problems.id =
      stageIdSchema.safeParse(id).error?.issues[0]?.message ??
      'A stage id is lower case words joined by "-"'
  } else if (usedIds(steps).has(id)) {
    problems.id = 'Another stage uses this id'
  } else if (catalog !== undefined) {
    problems.id = `${stageName(catalog.stage)} is a stage of the list. Add it from there, or pick another id`
  }
  const output = input.output.trim()
  if (input.kind === 'agent' && output !== '') {
    const message = stageSchema.shape.output.safeParse(output).error?.issues[0]?.message
    if (message !== undefined) {
      problems.output = message
    }
  }
  return problems
}

// A stage of your own waits for your approval until you change it.
export function addCustomStage(steps: readonly Step[], input: CustomStageInput): Step[] {
  if (Object.keys(customStageProblems(steps, input)).length > 0) {
    return steps as Step[]
  }
  const name = input.name.trim()
  const output = input.output.trim()
  const stage: Stage = {
    id: input.id,
    kind: input.kind,
    gate: 'manual',
    ...(name === stageName({ id: input.id }) ? {} : { name }),
    ...(input.kind === 'agent' && output !== '' ? { output } : {}),
  }
  return addBeforeRelease(steps, stage)
}

function updateStage(steps: readonly Step[], id: string, change: (stage: Stage) => Stage): Step[] {
  return steps.map((step) => {
    if (isBranch(step)) {
      return step.paths.some((path) => path.stages.some((stage) => stage.id === id))
        ? {
            ...step,
            paths: step.paths.map((path) => ({
              ...path,
              stages: path.stages.map((stage) => (stage.id === id ? change(stage) : stage)),
            })),
          }
        : step
    }
    if (isGroup(step)) {
      return step.parallel.some((stage) => stage.id === id)
        ? {
            ...step,
            parallel: step.parallel.map((stage) => (stage.id === id ? change(stage) : stage)),
          }
        : step
    }
    return step.id === id ? change(step) : step
  })
}

function updateBranch(
  steps: readonly Step[],
  branchId: string,
  change: (branch: StageBranch) => StageBranch,
): Step[] {
  return steps.map((step) => (isBranch(step) && step.id === branchId ? change(step) : step))
}

function updatePath(
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

// Sets a field, or removes it when the value is left out.
function withField<K extends keyof Stage>(
  stage: Stage,
  key: K,
  value: Stage[K] | undefined,
): Stage {
  const { [key]: _, ...rest } = stage
  return (value === undefined ? rest : { ...rest, [key]: value }) as Stage
}

// A stage that is off stays in the file with its settings; new tasks do not get it.
export function setEnabled(steps: readonly Step[], id: string, on: boolean): Step[] {
  return updateStage(steps, id, (stage) => withField(stage, 'enabled', on ? undefined : false))
}

export function setGate(steps: readonly Step[], id: string, gate: Gate): Step[] {
  return updateStage(steps, id, (stage) => ({ ...stage, gate }))
}

// Tasks get a "Run with a workflow" checkbox on the stage. Off leaves the field out.
export function setWorkflow(steps: readonly Step[], id: string, on: boolean): Step[] {
  return updateStage(steps, id, (stage) => withField(stage, 'workflow', on ? true : undefined))
}

export function setCi(steps: readonly Step[], id: string, on: boolean): Step[] {
  return updateStage(steps, id, (stage) => withField(stage, 'ci', on ? true : undefined))
}

// Off leaves the field out, so a stage of the catalog stays as the catalog has it.
export function setEndShell(steps: readonly Step[], id: string, on: boolean): Step[] {
  return updateStage(steps, id, (stage) => withField(stage, 'endShell', on ? true : undefined))
}

// The catalog offers CI on Implement and Automated tests; a stage of your own does not.
export function defaultCi(stage: Pick<Stage, 'id'>): boolean {
  return catalogEntry(stage.id)?.stage.ci === true
}

// The catalog offers a workflow on its hard stages; a stage of your own does not.
export function defaultWorkflow(stage: Pick<Stage, 'id' | 'kind'>): boolean {
  return stage.kind === 'agent' && catalogEntry(stage.id)?.stage.workflow === true
}

export type TextField =
  | 'name'
  | 'prompt'
  | 'command'
  | 'check'
  | 'output'
  | 'doneWhen'
  | 'agent'
  | 'args'

// `run` is the command of a path's condition; its `stage` is the path.
export type ShellField = {
  stage: Pick<Stage, 'id' | 'name'>
  field: 'check' | 'command' | 'args' | 'run'
  value: string
}

function stageFields(stage: Stage): ShellField[] {
  return (['check', 'command', 'args'] as const).flatMap((field) => {
    const value = stage[field]
    return value === undefined ? [] : [{ stage, field, value }]
  })
}

// What a settings file would run in a shell or put on an agent's command line, so an import can
// show it before it lands. Stages that are off count: they can be turned on later.
export function shellFields(steps: readonly Step[]): ShellField[] {
  return steps.flatMap((step) =>
    isBranch(step)
      ? step.paths.flatMap((path): ShellField[] => [
          ...(path.when?.type === 'command'
            ? [{ stage: path, field: 'run' as const, value: path.when.run }]
            : []),
          ...path.stages.flatMap(stageFields),
        ])
      : stagesOf(step).flatMap(stageFields),
  )
}

function stepOfStage(steps: readonly Step[], id: string): Step | undefined {
  return steps.find((candidate) => stagesOf(candidate).some((stage) => stage.id === id))
}

// A stage beside the stage in Shell 1 starts its own agent, so it may pick another one. So may a
// stage of your own, which then runs in a shell of its own.
export function takesOwnAgent(steps: readonly Step[], id: string): boolean {
  const step = stepOfStage(steps, id)
  const stage = step === undefined ? undefined : stagesOf(step).find((item) => item.id === id)
  return step !== undefined && stage !== undefined && canHaveOwnAgent(step, stage)
}

export function runsInShellOne(steps: readonly Step[], id: string): boolean {
  return stepOfStage(steps, id) !== undefined && terminalOfStage(steps, id) === '1'
}

// The default prompt as the agent gets it: the daemon puts the commit rule in `{commit}` of a
// default prompt only (see stage-prompts.ts), so a prompt that starts from this one keeps the rule.
export function shownDefaultPrompt(
  stage: Pick<Stage, 'id' | 'kind' | 'name' | 'output' | 'requireCommit'>,
): string {
  return defaultPrompt(stage).replaceAll('{commit}', commitRule(stage))
}

// An empty value removes the field. A name equal to the default name and the default prompt are
// left out too, so the file keeps only what differs.
export function setText(
  steps: readonly Step[],
  id: string,
  field: TextField,
  value: string,
): Step[] {
  return updateStage(steps, id, (stage) => {
    const empty = value.trim() === ''
    const isDefault =
      (field === 'name' && value === stageName({ id: stage.id })) ||
      (field === 'prompt' &&
        (value === defaultPrompt(stage) || value === shownDefaultPrompt(stage)))
    return withField(stage, field, empty || isDefault ? undefined : value)
  })
}

export function setSections(
  steps: readonly Step[],
  id: string,
  sections: readonly string[],
): Step[] {
  const kept = sections.map((section) => section.trim()).filter((section) => section !== '')
  return updateStage(steps, id, (stage) =>
    withField(stage, 'sections', kept.length === 0 ? undefined : kept),
  )
}

// Review and manual stages always wait for a person.
export function alwaysWaits(stage: Pick<Stage, 'kind'>): boolean {
  return stage.kind === 'review' || stage.kind === 'manual'
}

// The catalog's gate; a stage of your own waits for your approval.
export function defaultGate(stage: Pick<Stage, 'id' | 'kind'>): Gate {
  if (alwaysWaits(stage)) {
    return 'manual'
  }
  return catalogEntry(stage.id)?.stage.gate ?? 'manual'
}

export function isDefaultGate(stage: Pick<Stage, 'id' | 'kind' | 'gate'>): boolean {
  return stage.gate === defaultGate(stage)
}

export function hasCustomPrompt(stage: Pick<Stage, 'prompt'>): boolean {
  return stage.prompt !== undefined
}

export function isCatalogStage(id: string): boolean {
  return catalogEntry(id) !== undefined
}

// Agent and testing stages get a prompt.
export function takesPrompt(stage: Pick<Stage, 'kind'>): boolean {
  return stage.kind === 'agent' || stage.kind === 'testing'
}

export const KIND_LABEL: Record<StageKind, string> = {
  context: 'Context',
  agent: 'Agent',
  review: 'Code review',
  testing: 'Testing',
  release: 'Release',
  manual: 'Check by a person',
}

export function approvalLabel(stage: Pick<Stage, 'kind' | 'gate'>): string {
  if (alwaysWaits(stage)) {
    return 'Always waits for you'
  }
  if (stage.kind === 'release') {
    return stage.gate === 'manual'
      ? 'Opens the pull request when you click'
      : 'Opens the pull request by itself'
  }
  return stage.gate === 'manual' ? 'Waits for your approval' : 'Moves on by itself'
}

// "A, B and C"
export function joinNames(names: readonly string[]): string {
  if (names.length <= 1) {
    return names[0] ?? ''
  }
  return `${names.slice(0, -1).join(', ')} and ${names.at(-1)}`
}

export function groupName(group: StageGroup): string {
  return group.name ?? joinNames(group.parallel.map(stageName))
}

export function branchName(branch: Pick<StageBranch, 'id' | 'name'>): string {
  return stageName(branch)
}

export function pathName(path: Pick<BranchPath, 'id' | 'name'>): string {
  return stageName(path)
}

// The name of a step in a sentence: a stage, a group or a branch.
export function stepName(step: Step): string {
  if (isGroup(step)) {
    return groupName(step)
  }
  return isBranch(step) ? branchName(step) : stageName(step)
}

// "Small change if plan.md contains "small"", or "Review PR, otherwise".
export function pathLabel(path: BranchPath): string {
  return path.when === undefined
    ? `${pathName(path)}, otherwise`
    : `${pathName(path)} if ${describeCondition(path)}`
}

// "After plan, one path of 2: Small change if …; Review PR, otherwise"
export function branchLabel(branch: StageBranch): string {
  return `${branchName(branch)}, one path of ${branch.paths.length}: ${branch.paths.map(pathLabel).join('; ')}`
}

// What the preview says to a screen reader: the stages that are on, in order.
export function pipelineLabel(steps: readonly Step[]): string {
  const parts = activeSteps(steps).map((step) => {
    if (isBranch(step)) {
      return branchLabel(step)
    }
    return isGroup(step)
      ? `${joinNames(step.parallel.map(stageName))} side by side`
      : stageName(step)
  })
  return parts.length === 0 ? 'Pipeline: no stage is on' : `Pipeline: ${parts.join(', then ')}`
}
