import {
  type CreateIssueWhen,
  DEFAULT_PIPELINE,
  DEFAULT_PROJECT_SETTINGS,
  GATES,
  projectSettingsSchema,
  STAGE_KINDS,
  type Step,
  trackerSettingsSchema,
} from '@natamux/core'

export type RawSettings = Record<string, unknown>

export type Draft = { ok: true; raw: RawSettings; steps: Step[] } | { ok: false; error: string }

export const ID_RULE = /^[a-z0-9]+(-[a-z0-9]+)*$/

export const MAX_ID = 40

export const MAX_GROUP = 4

export const MAX_PATHS = 4

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
