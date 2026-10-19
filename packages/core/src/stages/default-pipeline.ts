import { z } from 'zod'
import { isBranch, isGroup, type Stage, type Step, stageIdSchema, stagesOf } from './schema.js'
import { catalogEntry, STAGE_CATALOG } from './stage-catalog.js'

// A stage of the catalog, as a pipeline holds it.
function catalogStage(id: string): Stage {
  const entry = catalogEntry(id)
  if (entry === undefined) {
    throw new Error(`The stage catalog has no stage "${id}"`)
  }
  return entry.stage
}

export const DEFAULT_PIPELINE: Step[] = [
  catalogStage('context'),
  catalogStage('spec'),
  catalogStage('plan'),
  {
    id: 'build',
    name: 'Test spec and Implement',
    parallel: [catalogStage('test-spec'), catalogStage('implement')],
  },
  // Tests run before the review, and an agent reviews; it sends the task back to Implement on its
  // own. Manual review, by a person, is in the catalog.
  catalogStage('testing'),
  catalogStage('agent-review'),
  catalogStage('release'),
]

// What a task made before stages existed goes through: the work it had then.
export const LEGACY_PIPELINE: Step[] = [
  { id: 'implement', kind: 'agent', gate: 'auto' },
  { id: 'review', kind: 'review', gate: 'manual' },
  { id: 'release', kind: 'release', gate: 'manual' },
]

// What a task keeps of its pipeline: the ids, in order. Names, gates and checks are read from the
// current settings, so they can change while the task is on its way.
export const stepRefSchema = z.union([
  stageIdSchema,
  z.object({ id: stageIdSchema, parallel: z.array(stageIdSchema).min(2).max(4) }),
  z.object({
    id: stageIdSchema,
    paths: z
      .array(z.object({ id: stageIdSchema, stages: z.array(stageIdSchema).min(1) }))
      .min(2)
      .max(4),
  }),
])

export type StepRef = z.infer<typeof stepRefSchema>

export function stepRefsOf(steps: readonly Step[]): StepRef[] {
  return steps.map((step) => {
    if (isBranch(step)) {
      const paths = step.paths.map((path) => ({
        id: path.id,
        stages: path.stages.map((stage) => stage.id),
      }))
      return { id: step.id, paths }
    }
    return isGroup(step)
      ? { id: step.id, parallel: step.parallel.map((stage) => stage.id) }
      : step.id
  })
}

export const LEGACY_STEP_REFS: StepRef[] = stepRefsOf(LEGACY_PIPELINE)

// Turns a task's step ids back into steps, with each stage's settings from `pipeline`. A stage
// that is gone from the settings keeps the catalog settings of its id, so a task on its way still
// runs it; a stage of its own that is gone becomes a manual stage, so the task can go on. A stage
// that is off still runs for a task that has it.
export function resolveSteps(refs: readonly StepRef[], pipeline: readonly Step[]): Step[] {
  const known = new Map<string, Stage>()
  const stages = [...STAGE_CATALOG.map((entry) => entry.stage), ...pipeline.flatMap(stagesOf)]
  for (const stage of stages) {
    known.set(stage.id, stage)
  }
  const stage = (id: string): Stage => known.get(id) ?? { id, kind: 'manual', gate: 'manual' }
  return refs.map((ref): Step => {
    if (typeof ref === 'string') {
      return stage(ref)
    }
    const step = pipeline.find((each) => each.id === ref.id)
    const named = step?.name === undefined ? {} : { name: step.name }
    if ('parallel' in ref) {
      return { id: ref.id, ...named, parallel: ref.parallel.map(stage) }
    }
    // Conditions come from the settings too. A path that is gone from them keeps its stages and
    // has no condition.
    const branch = step !== undefined && isBranch(step) ? step : undefined
    const paths = ref.paths.map((path) => {
      const now = branch?.paths.find((each) => each.id === path.id)
      return {
        id: path.id,
        ...(now?.name === undefined ? {} : { name: now.name }),
        ...(now?.when === undefined ? {} : { when: now.when }),
        ...(now?.not === undefined ? {} : { not: now.not }),
        stages: path.stages.map(stage),
      }
    })
    return { id: ref.id, ...named, paths }
  })
}
