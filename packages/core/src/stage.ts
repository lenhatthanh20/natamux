import { z } from 'zod'
import { catalogEntry, STAGE_CATALOG } from './stage-catalog.js'
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
  // A stage that is off stays in the file with its settings, but new tasks do not get it.
  enabled: z.boolean().optional(),
  // The stage offers its tasks a checkbox to run it as a workflow (agent stages only).
  workflow: z.boolean().optional(),
})

export type Stage = z.infer<typeof stageSchema>

export function isOn(stage: Pick<Stage, 'enabled'>): boolean {
  return stage.enabled !== false
}

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

// The steps that new tasks get: stages that are off are left out, and so is a group with none on.
export function activeSteps(steps: readonly Step[]): Step[] {
  return steps.flatMap((step): Step[] => {
    if (!isGroup(step)) {
      return isOn(step) ? [step] : []
    }
    const on = step.parallel.filter(isOn)
    if (on.length === 0) {
      return []
    }
    // The rules ask for two stages that are on; one left over runs on its own.
    return on.length === 1 ? on : [{ ...step, parallel: on }]
  })
}

export const pipelineSchema = z
  .array(stepSchema)
  .min(1, 'A pipeline has at least one stage')
  .superRefine((steps, context) => {
    const seen = new Set<string>()
    steps.forEach((step, index) => {
      const ids = isGroup(step) ? [step.id, ...step.parallel.map((stage) => stage.id)] : [step.id]
      for (const id of ids) {
        if (seen.has(id)) {
          context.addIssue({
            code: 'custom',
            message: `The stage id "${id}" is used twice`,
            path: [index],
          })
        }
        seen.add(id)
      }
      for (const stage of stagesOf(step)) {
        if (stage.workflow === true && stage.kind !== 'agent') {
          context.addIssue({
            code: 'custom',
            message: `Only agent stages can run with a workflow, and "${stage.id}" is a ${stage.kind} stage`,
            path: [index],
          })
        }
      }
      if (isGroup(step) && step.parallel.filter(isOn).length === 1) {
        context.addIssue({
          code: 'custom',
          message: 'A group needs 2 stages that are on. Turn on another, or run it on its own',
          path: [index],
        })
      }
    })
    // Only stages that are on count from here: the others are not in new tasks.
    const on = steps
      .map((step, index) => ({ step, index }))
      .filter(({ step }) => (isGroup(step) ? step.parallel.some(isOn) : isOn(step)))
    if (on.length === 0) {
      context.addIssue({ code: 'custom', message: 'Turn on at least one stage' })
      return
    }
    // The context is read when a task is made, so only the first step can be one. A release
    // ends the task, so nothing can come after it.
    const kindAt = ({ step }: (typeof on)[number]) => (isGroup(step) ? undefined : step.kind)
    const contexts = on.filter((entry) => kindAt(entry) === 'context')
    const releases = on.filter((entry) => kindAt(entry) === 'release')
    if (contexts.length > 1) {
      context.addIssue({
        code: 'custom',
        message: 'A pipeline has at most one context stage',
        path: [contexts[1]?.index ?? 0],
      })
    } else if (contexts[0] !== undefined && contexts[0] !== on[0]) {
      context.addIssue({
        code: 'custom',
        message: 'The context stage must come first',
        path: [contexts[0].index],
      })
    }
    if (releases.length > 1) {
      context.addIssue({
        code: 'custom',
        message: 'A pipeline has at most one release stage',
        path: [releases[1]?.index ?? 0],
      })
    } else if (releases[0] !== undefined && releases[0] !== on.at(-1)) {
      context.addIssue({
        code: 'custom',
        message: 'The release stage must come last',
        path: [releases[0].index],
      })
    }
  })

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
  catalogStage('review'),
  catalogStage('testing'),
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
])

export type StepRef = z.infer<typeof stepRefSchema>

export function stepRefsOf(steps: readonly Step[]): StepRef[] {
  return steps.map((step) =>
    isGroup(step) ? { id: step.id, parallel: step.parallel.map((stage) => stage.id) } : step.id,
  )
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

// Agents that split a stage across sub-agents when the prompt asks for a workflow.
export const WORKFLOW_AGENTS: readonly string[] = ['claude-code']

// A task with no agent named gets Claude Code.
export function canRunWorkflow(agent: string | null): boolean {
  return WORKFLOW_AGENTS.includes(agent ?? 'claude-code')
}

// The project settings let tasks run this stage as a workflow.
export function offersWorkflow(stage: Pick<Stage, 'kind' | 'workflow'>): boolean {
  return stage.kind === 'agent' && stage.workflow === true
}

// Whether the next start of the stage asks the agent for a workflow: the settings offer it, the
// task chose it, and the agent can run one.
export function runsWorkflow(
  stage: Pick<Stage, 'id' | 'kind' | 'workflow'>,
  chosen: readonly string[],
  agent: string | null,
): boolean {
  return offersWorkflow(stage) && chosen.includes(stage.id) && canRunWorkflow(agent)
}

export function stageName(stage: Pick<Stage, 'id' | 'name'>): string {
  if (stage.name !== undefined) {
    return stage.name
  }
  const named = catalogEntry(stage.id)?.stage.name
  if (named !== undefined) {
    return named
  }
  const words = stage.id.replaceAll('-', ' ')
  return words.charAt(0).toUpperCase() + words.slice(1)
}

// What a prompt may hold. The daemon fills them when the stage starts.
export const PROMPT_PLACEHOLDERS = ['title', 'description', 'dir', 'ref'] as const

// `{skill:<name>}` in a prompt: a skill, called the way the task's agent calls it. A plugin skill
// is named `<plugin>:<skill>`.
export const SKILL_PLACEHOLDER = /\{skill:([\w.-]+(?::[\w.-]+)?)\}/g

export function skillPlaceholder(name: string): string {
  return `{skill:${name}}`
}

// By default you read the changes before they are committed. A stage that checks for a commit
// has to ask for one, or the agent can never pass it.
export function commitRule(stage: Pick<Stage, 'requireCommit'>): string {
  return stage.requireCommit === true
    ? 'Commit your work when it is done.'
    : 'Do not commit. Leave the changes for review.'
}

// The prompt of an agent or testing stage when the settings give none. The daemon turns
// `{commit}` into `commitRule` of the stage.
export function defaultPrompt(stage: Pick<Stage, 'id' | 'kind' | 'name' | 'output'>): string {
  // A stage of a project's own may have the id of a catalog stage that came later; it keeps the
  // catalog prompt only when the prompt fits it.
  const entry = catalogEntry(stage.id)
  const fits =
    entry?.stage.kind === stage.kind &&
    (stage.output === undefined || stage.output === entry.stage.output)
  if (fits && entry?.prompt !== undefined) {
    return entry.prompt
  }
  return [
    'Task: {title}',
    `Do the ${stageName(stage)} stage of this task.`,
    ...(stage.output === undefined ? [] : [`Write {dir}/${stage.output}.`]),
  ].join('\n')
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

// The stage an agent terminal works on, and the state of that stage. Both are null for a terminal
// that no stage uses, such as an extra shell where the user started an agent.
export const terminalLaneSchema = z.object({
  stage: z.string().min(1).nullable(),
  state: taskStateSchema.nullable(),
  // The agent that runs in the terminal, which may differ from the task's. Null: a program that is
  // no agent Natamux knows. Missing: not known.
  agent: z.string().min(1).nullable().optional(),
})

export type TerminalLane = z.infer<typeof terminalLaneSchema>

const NO_LANE: TerminalLane = { stage: null, state: null }

// In a group each terminal has the state of its own lane, not the state of the whole task.
export function terminalLane(
  position: StagePosition,
  step: Step | undefined,
  terminal: string,
): TerminalLane {
  if (step === undefined || !isGroup(step)) {
    const stage = step?.id ?? position.stage
    return terminal === '1' ? { stage, state: position.state } : NO_LANE
  }
  const stage = stageOfTerminal(step, terminal)
  if (stage === undefined) {
    return NO_LANE
  }
  return { stage: stage.id, state: position.lanes[stage.id] ?? 'done' }
}

export const lanesSchema = z.record(stageIdSchema, taskStateSchema)
