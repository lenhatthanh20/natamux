import {
  activeSteps,
  type CatalogStage,
  type CreateIssueWhen,
  canHaveOwnAgent,
  catalogEntry,
  commitRule,
  DEFAULT_PIPELINE,
  DEFAULT_PROJECT_SETTINGS,
  defaultPrompt,
  GATES,
  type Gate,
  GROUP_KINDS,
  isGroup,
  projectSettingsSchema,
  STAGE_CATALOG,
  STAGE_KINDS,
  type Stage,
  type StageGroup,
  type StageKind,
  type Step,
  stageIdSchema,
  stageName,
  stageSchema,
  stagesOf,
  trackerSettingsSchema,
} from '@natamux/core'

// The logic of the Stages tab of Project settings. The draft is the text of `.natamux.json`; the
// tab reads `stages` from it and writes them back, and keeps every other key as it is.

export type RawSettings = Record<string, unknown>

export type Draft = { ok: true; raw: RawSettings; steps: Step[] } | { ok: false; error: string }

const ID_RULE = /^[a-z0-9]+(-[a-z0-9]+)*$/
const MAX_ID = 40
const MAX_GROUP = 4

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

// Every id in the pipeline: stages and groups.
export function usedIds(steps: readonly Step[]): Set<string> {
  return new Set(
    steps.flatMap((step) =>
      isGroup(step) ? [step.id, ...step.parallel.map((stage) => stage.id)] : [step.id],
    ),
  )
}

type Place = { index: number; lane: number | null }

// Where a stage is: its step, and its lane when it runs in a group. A group's own id has no lane.
export function findPlace(steps: readonly Step[], id: string): Place | null {
  for (const [index, step] of steps.entries()) {
    if (step.id === id) {
      return { index, lane: null }
    }
    if (isGroup(step)) {
      const lane = step.parallel.findIndex((stage) => stage.id === id)
      if (lane !== -1) {
        return { index, lane }
      }
    }
  }
  return null
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
  return replaceAt(replaceAt(steps, target, step), place.index, other)
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
  if (
    place.lane !== null &&
    isGroup(host) &&
    !isGroup(taken) &&
    GROUP_KINDS.includes(taken.kind) &&
    host.parallel.length < MAX_GROUP
  ) {
    const parallel = [...host.parallel]
    parallel.splice(side === 'before' ? place.lane : place.lane + 1, 0, taken)
    return replaceAt(out.rest, place.index, { ...host, parallel })
  }
  return insertAt(out.rest, side === 'before' ? place.index : place.index + 1, taken)
}

// A group id that no stage or group uses yet, from the id of its first stage.
function newGroupId(steps: readonly Step[], first: string): string {
  const used = usedIds(steps)
  const base = first.length <= MAX_ID - 10 ? `${first}-group` : 'group'
  if (!used.has(base)) {
    return base
  }
  let number = 2
  while (used.has(`${base}-${number}`)) {
    number += 1
  }
  return `${base}-${number}`
}

// Why a stage cannot run alongside the stage above, or null when it can.
export function joinProblem(steps: readonly Step[], id: string): string | null {
  const place = findPlace(steps, id)
  const step = place === null ? undefined : steps[place.index]
  if (place !== null && place.lane !== null) {
    return 'It runs alongside other stages already'
  }
  if (place === null || step === undefined || isGroup(step)) {
    return 'Only a stage can run alongside another'
  }
  if (!GROUP_KINDS.includes(step.kind)) {
    return 'Only agent and manual stages run side by side'
  }
  const above = steps[place.index - 1]
  if (above === undefined) {
    return 'There is no stage above'
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
  const release = steps.findIndex((step) => !isGroup(step) && step.kind === 'release')
  return insertAt(steps, release === -1 ? steps.length : release, stage)
}

export function addCatalogStage(steps: readonly Step[], id: string): Step[] {
  const entry = catalogEntry(id)
  if (entry === undefined || usedIds(steps).has(id)) {
    return steps as Step[]
  }
  const { sections, ...stage } = entry.stage
  return addBeforeRelease(
    steps,
    sections === undefined ? stage : { ...stage, sections: [...sections] },
  )
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

// The catalog offers CI on Implement and Automated tests; a stage of your own does not.
export function defaultCi(stage: Pick<Stage, 'id'>): boolean {
  return catalogEntry(stage.id)?.stage.ci === true
}

// The catalog offers a workflow on its hard stages; a stage of your own does not.
export function defaultWorkflow(stage: Pick<Stage, 'id' | 'kind'>): boolean {
  return stage.kind === 'agent' && catalogEntry(stage.id)?.stage.workflow === true
}

export type TextField = 'name' | 'prompt' | 'command' | 'output' | 'doneWhen' | 'agent' | 'args'

// A stage beside the stage in Shell 1 starts its own agent, so it may pick another one.
export function takesOwnAgent(steps: readonly Step[], id: string): boolean {
  const step = steps.find((candidate) => stagesOf(candidate).some((stage) => stage.id === id))
  const stage = step === undefined ? undefined : stagesOf(step).find((item) => item.id === id)
  return step !== undefined && stage !== undefined && canHaveOwnAgent(step, stage)
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

// What the preview says to a screen reader: the stages that are on, in order.
export function pipelineLabel(steps: readonly Step[]): string {
  const parts = activeSteps(steps).map((step) =>
    isGroup(step) ? `${joinNames(step.parallel.map(stageName))} side by side` : stageName(step),
  )
  return parts.length === 0 ? 'Pipeline: no stage is on' : `Pipeline: ${parts.join(', then ')}`
}
