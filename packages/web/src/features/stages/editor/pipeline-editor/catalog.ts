import {
  type CatalogStage,
  catalogEntry,
  type PipelineIssue,
  STAGE_CATALOG,
  type Stage,
  type StageKind,
  type Step,
  stageIdSchema,
  stageName,
  stageSchema,
} from '@natamux/core'
import { updatePath } from './branches'
import { findPath, fitsInPath, insertAt, releaseIndex, usedIds } from './find'
import { ID_RULE, MAX_ID } from './settings-json'

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

// New stages go before the release, which must stay last. A context stage must come first.
function addBeforeRelease(steps: readonly Step[], stage: Stage): Step[] {
  if (stage.kind === 'context') {
    return insertAt(steps, 0, stage)
  }
  return insertAt(steps, releaseIndex(steps), stage)
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

export function isCatalogStage(id: string): boolean {
  return catalogEntry(id) !== undefined
}
