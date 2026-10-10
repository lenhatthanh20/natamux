import { z } from 'zod'
import { type TaskState, taskStateSchema } from './task-state.js'

// See docs/design-task-stages.md for the model: stages, groups (parallel steps) and lanes.

export const STAGE_KINDS = ['context', 'agent', 'review', 'testing', 'release', 'manual'] as const
export type StageKind = (typeof STAGE_KINDS)[number]

export const GATES = ['manual', 'auto'] as const
export type Gate = (typeof GATES)[number]

// Kinds that may run side by side in a group.
export const GROUP_KINDS: readonly StageKind[] = ['agent', 'manual']

export const stageIdSchema = z
  .string()
  .regex(/^[a-z0-9]+(-[a-z0-9]+)*$/, 'A stage id is lower case words joined by "-"')
  .max(40)

export const stageSchema = z.object({
  id: stageIdSchema,
  kind: z.enum(STAGE_KINDS),
  gate: z.enum(GATES),
  name: z.string().trim().min(1).max(60).optional(),
  prompt: z.string().trim().min(1).optional(),
  // A file the stage must write, in the task's stage file folder.
  output: z
    .string()
    .regex(/^[\w-]+(\.[\w-]+)*\.md$/, 'An output is a markdown file name, such as spec.md')
    .optional(),
  sections: z.array(z.string().trim().min(1)).optional(),
  requireCommit: z.boolean().optional(),
  requireClean: z.boolean().optional(),
  check: z.string().trim().min(1).optional(),
  command: z.string().trim().min(1).optional(),
  doneWhen: z.string().trim().min(1).optional(),
})

export type Stage = z.infer<typeof stageSchema>

export const stageGroupSchema = z.object({
  id: stageIdSchema,
  name: z.string().trim().min(1).max(60).optional(),
  parallel: z
    .array(
      stageSchema.refine((stage) => GROUP_KINDS.includes(stage.kind), {
        message: 'Only agent and manual stages can run side by side',
        path: ['kind'],
      }),
    )
    .min(2, 'A group holds 2 to 4 stages')
    .max(4, 'A group holds 2 to 4 stages'),
})

export type StageGroup = z.infer<typeof stageGroupSchema>

// A step of the pipeline: one stage, or a group of stages that run at the same time.
export const stepSchema = z.union([stageGroupSchema, stageSchema])
export type Step = z.infer<typeof stepSchema>

export function isGroup(step: Step): step is StageGroup {
  return 'parallel' in step
}

// The stages of a step: itself, or the stages of its group.
export function stagesOf(step: Step): Stage[] {
  return isGroup(step) ? step.parallel : [step]
}

export const pipelineSchema = z
  .array(stepSchema)
  .min(1, 'A pipeline has at least one stage')
  .superRefine((steps, context) => {
    const seen = new Set<string>()
    for (const id of steps.flatMap((step) => [
      step.id,
      ...(isGroup(step) ? step.parallel.map((stage) => stage.id) : []),
    ])) {
      if (seen.has(id)) {
        context.addIssue({ code: 'custom', message: `The stage id "${id}" is used twice` })
      }
      seen.add(id)
    }
    // The context is read when a task is made, so only the first step can be one. A release
    // ends the task, so nothing can come after it.
    const kinds = steps.map((step) => (isGroup(step) ? undefined : step.kind))
    if (kinds.filter((kind) => kind === 'context').length > 1) {
      context.addIssue({ code: 'custom', message: 'A pipeline has at most one context stage' })
    } else if (kinds.indexOf('context') > 0) {
      context.addIssue({ code: 'custom', message: 'The context stage must come first' })
    }
    if (kinds.filter((kind) => kind === 'release').length > 1) {
      context.addIssue({ code: 'custom', message: 'A pipeline has at most one release stage' })
    } else if (kinds.includes('release') && kinds.at(-1) !== 'release') {
      context.addIssue({ code: 'custom', message: 'The release stage must come last' })
    }
  })

export const DEFAULT_PIPELINE: Step[] = [
  { id: 'context', kind: 'context', gate: 'manual', output: 'context.md' },
  {
    id: 'spec',
    kind: 'agent',
    gate: 'manual',
    output: 'spec.md',
    sections: ['Goal', 'Scope', 'Out of scope', 'Acceptance criteria'],
  },
  {
    id: 'plan',
    kind: 'agent',
    gate: 'manual',
    output: 'plan.md',
    sections: ['Steps', 'Files', 'Tests'],
  },
  {
    id: 'build',
    name: 'Test spec and Implement',
    parallel: [
      {
        id: 'test-spec',
        kind: 'agent',
        gate: 'manual',
        output: 'test-spec.md',
        sections: ['Cases'],
      },
      // No commit check: the agent leaves its changes for you to read before you commit them.
      { id: 'implement', kind: 'agent', gate: 'auto' },
    ],
  },
  { id: 'review', kind: 'review', gate: 'manual' },
  { id: 'testing', kind: 'testing', gate: 'auto' },
  { id: 'release', kind: 'release', gate: 'manual' },
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
])

export type StepRef = z.infer<typeof stepRefSchema>

export function stepRefsOf(steps: readonly Step[]): StepRef[] {
  return steps.map((step) =>
    isGroup(step) ? { id: step.id, parallel: step.parallel.map((stage) => stage.id) } : step.id,
  )
}

export const LEGACY_STEP_REFS: StepRef[] = stepRefsOf(LEGACY_PIPELINE)

// Turns a task's step ids back into steps, with each stage's settings from `pipeline`. A stage
// that is gone from the settings keeps the default settings of its id, so a task on its way still
// runs it; a stage of its own that is gone becomes a manual stage, so the task can go on.
export function resolveSteps(refs: readonly StepRef[], pipeline: readonly Step[]): Step[] {
  const known = new Map<string, Stage>()
  for (const step of [...DEFAULT_PIPELINE, ...pipeline]) {
    for (const stage of stagesOf(step)) {
      known.set(stage.id, stage)
    }
  }
  const stage = (id: string): Stage => known.get(id) ?? { id, kind: 'manual', gate: 'manual' }
  return refs.map((ref) => {
    if (typeof ref === 'string') {
      return stage(ref)
    }
    const group = pipeline.find((step) => step.id === ref.id)
    return {
      id: ref.id,
      ...(group?.name === undefined ? {} : { name: group.name }),
      parallel: ref.parallel.map(stage),
    }
  })
}

export function stageName(stage: Pick<Stage, 'id' | 'name'>): string {
  if (stage.name !== undefined) {
    return stage.name
  }
  const words = stage.id.replaceAll('-', ' ')
  return words.charAt(0).toUpperCase() + words.slice(1)
}

// Where a task is: its step, the state of that step, and for a group the state of each stage.
export type StagePosition = {
  stage: string
  state: TaskState
  lanes: Record<string, TaskState>
}

// The lane states, the most urgent first. A group's state is its most urgent lane.
const URGENCY: readonly TaskState[] = [
  'needs-input',
  'failed',
  'blocked',
  'review',
  'running',
  'ready',
  'draft',
]

export function combinedState(lanes: Readonly<Record<string, TaskState>>): TaskState {
  const states = Object.values(lanes)
  return URGENCY.find((state) => states.includes(state)) ?? 'done'
}

// The state a stage starts in. A stage that only waits for a person waits at once. A release
// waits for its pull request to be merged, which is not a wait for you.
export function entryState(stage: Pick<Stage, 'kind'>): TaskState {
  return stage.kind === 'review' || stage.kind === 'manual' ? 'review' : 'ready'
}

export function enterStep(step: Step, state?: TaskState): StagePosition {
  if (!isGroup(step)) {
    return { stage: step.id, state: state ?? entryState(step), lanes: {} }
  }
  const lanes = Object.fromEntries(
    step.parallel.map((stage) => [stage.id, state ?? entryState(stage)]),
  )
  return { stage: step.id, state: combinedState(lanes), lanes }
}

// A new task waits at its first step until it is readied.
export function firstPosition(steps: readonly Step[]): StagePosition {
  const [first] = steps
  if (first === undefined) {
    return { stage: 'implement', state: 'draft', lanes: {} }
  }
  return enterStep(first, 'draft')
}

export function findStep(steps: readonly Step[], id: string): Step | undefined {
  return steps.find((step) => step.id === id || stagesOf(step).some((stage) => stage.id === id))
}

export function stepIndex(steps: readonly Step[], id: string): number {
  return steps.findIndex(
    (step) => step.id === id || stagesOf(step).some((stage) => stage.id === id),
  )
}

// The stage of a group that changes code keeps the first terminal (and the agent session of the
// steps before); each other stage gets the next terminal.
export function codeStage(step: Step): Stage {
  const stages = stagesOf(step)
  const agents = stages.filter((stage) => stage.kind === 'agent')
  return agents.find((stage) => stage.output === undefined) ?? agents[0] ?? (stages[0] as Stage)
}

export function terminalOfStage(step: Step, stageId: string): string {
  if (!isGroup(step)) {
    return '1'
  }
  const main = codeStage(step)
  if (main.id === stageId) {
    return '1'
  }
  const others = step.parallel.filter((stage) => stage.id !== main.id)
  return String(others.findIndex((stage) => stage.id === stageId) + 2)
}

export function stageOfTerminal(step: Step, terminal: string): Stage | undefined {
  return stagesOf(step).find((stage) => terminalOfStage(step, stage.id) === terminal)
}

export const lanesSchema = z.record(stageIdSchema, taskStateSchema)
