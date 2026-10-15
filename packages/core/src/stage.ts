import { z } from 'zod'
import { catalogEntry, STAGE_CATALOG } from './stage-catalog.js'
import { type TaskState, taskStateSchema } from './task-state.js'

// See docs/design-task-stages.md for the model: stages, groups (parallel steps), branches (paths
// picked by a condition) and lanes.

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

const MARKDOWN_FILE = /^[\w-]+(\.[\w-]+)*\.md$/

export const stageSchema = z.object({
  id: stageIdSchema,
  kind: z.enum(STAGE_KINDS),
  gate: z.enum(GATES),
  name: z.string().trim().min(1).max(60).optional(),
  prompt: z.string().trim().min(1).optional(),
  // A file the stage must write, in the task's stage file folder.
  output: z
    .string()
    .regex(MARKDOWN_FILE, 'An output is a markdown file name, such as spec.md')
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
  // Another agent, and words added to its command line, for a stage that starts its own agent
  // beside the stage in Shell 1, or for a stage of your own. Such a stage gets a shell of its own;
  // Shell 1 keeps the task's agent, so its session goes on.
  agent: z.string().trim().min(1).optional(),
  args: z.string().trim().min(1).optional(),
  // The stage offers its tasks a checkbox to check the work on CI before the stage passes.
  ci: z.boolean().optional(),
  // A stage that only writes a report, such as a review, runs in its own shell with a new session:
  // it sees the work with fresh eyes, and may use another agent.
  ownShell: z.boolean().optional(),
  // Ends the agent or command of the stage, and closes its shell, when the stage is done. The
  // next stage in that shell then starts a new agent session.
  endShell: z.boolean().optional(),
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

// What picks a path of a branch: a stage file has a text, or a command exits with code 0.
export const conditionSchema = z.discriminatedUnion('type', [
  z.object({
    type: z.literal('output'),
    file: z.string().regex(MARKDOWN_FILE, 'A condition file is a markdown file, such as plan.md'),
    // Only the lines under this heading count.
    section: z.string().trim().min(1).optional(),
    contains: z.string().trim().min(1, 'Give the text to look for'),
  }),
  z.object({
    type: z.literal('command'),
    run: z.string().trim().min(1, 'Give the command to run'),
  }),
])

export type Condition = z.infer<typeof conditionSchema>

// One way through a branch. A path without `when` is the Otherwise path; `not` takes the path when
// the condition does not hold.
export const pathSchema = z.object({
  id: stageIdSchema,
  name: z.string().trim().min(1).max(60).optional(),
  when: conditionSchema.optional(),
  not: z.boolean().optional(),
  stages: z.array(stageSchema).min(1, 'A path has at least one stage'),
})

export type BranchPath = z.infer<typeof pathSchema>

// A step where a task takes one of its paths: the first whose condition holds.
export const branchSchema = z.object({
  id: stageIdSchema,
  name: z.string().trim().min(1).max(60).optional(),
  paths: z
    .array(pathSchema)
    .min(2, 'A branch holds 2 to 4 paths')
    .max(4, 'A branch holds 2 to 4 paths'),
})

export type StageBranch = z.infer<typeof branchSchema>

// A step of the pipeline: one stage, a group of stages that run at the same time, or a branch.
export const stepSchema = z.union([stageGroupSchema, branchSchema, stageSchema])
export type Step = z.infer<typeof stepSchema>

export function isGroup(step: Step): step is StageGroup {
  return 'parallel' in step
}

export function isBranch(step: Step): step is StageBranch {
  return 'paths' in step
}

export function isStage(step: Step): step is Stage {
  return !isGroup(step) && !isBranch(step)
}

// The stages of a step: itself, the stages of its group, or the stages of every path in order.
export function stagesOf(step: Step): Stage[] {
  if (isBranch(step)) {
    return step.paths.flatMap((path) => path.stages)
  }
  return isGroup(step) ? step.parallel : [step]
}

// The steps that new tasks get: stages that are off are left out, and so is a group with none on.
// A path with none on is left out of its branch; a branch left with only an Otherwise path runs
// its stages as plain steps.
export function activeSteps(steps: readonly Step[]): Step[] {
  return steps.flatMap((step): Step[] => {
    if (isBranch(step)) {
      const paths = step.paths.flatMap((path) => {
        const on = path.stages.filter(isOn)
        return on.length === 0 ? [] : [{ ...path, stages: on }]
      })
      const [only] = paths
      if (only === undefined) {
        return []
      }
      return paths.length === 1 && only.when === undefined ? only.stages : [{ ...step, paths }]
    }
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

// Each stage of a step, with the issue path that names it: its lane in a group, itself in a path.
function placesOf(step: Step, index: number): { stage: Stage; at: (string | number)[] }[] {
  if (isBranch(step)) {
    return step.paths.flatMap((path, p) =>
      path.stages.map((stage, s) => ({ stage, at: [index, 'paths', p, 'stages', s] })),
    )
  }
  return stagesOf(step).map((stage, lane) => ({
    stage,
    at: isGroup(step) ? [index, 'parallel', lane] : [index],
  }))
}

export const pipelineSchema = z
  .array(stepSchema)
  .min(1, 'A pipeline has at least one stage')
  .superRefine((steps, context) => {
    const seen = new Set<string>()
    const unique = (id: string, message: string, path: (string | number)[]) => {
      if (seen.has(id)) {
        context.addIssue({ code: 'custom', message, path })
      }
      seen.add(id)
    }
    steps.forEach((step, index) => {
      unique(step.id, `The stage id "${step.id}" is used twice`, [index])
      if (isBranch(step)) {
        const last = step.paths.length - 1
        step.paths.forEach((path, p) => {
          unique(path.id, `The path id "${path.id}" is used twice`, [index, 'paths', p])
          if (path.when === undefined && p !== last) {
            context.addIssue({
              code: 'custom',
              message: 'Only the last path can be the Otherwise path',
              path: [index, 'paths', p],
            })
          }
        })
        // A path with all stages off is left out of new tasks. Only an Otherwise path can be the
        // one left, since its stages then run as plain steps.
        const filled = step.paths.filter((path) => path.stages.length > 0)
        const left = filled.filter((path) => path.stages.some(isOn))
        if (filled.length >= 2 && left.length === 1 && left[0]?.when !== undefined) {
          context.addIssue({
            code: 'custom',
            message: 'Turn on a stage in another path, or remove the branch',
            path: [index],
          })
        }
      }
      for (const { stage, at } of placesOf(step, index)) {
        // Outside a branch these rules name the whole step, as they did before branches.
        const named = isBranch(step) ? at : [index]
        if (isGroup(step) || isBranch(step)) {
          unique(stage.id, `The stage id "${stage.id}" is used twice`, named)
        }
        if (stage.ownShell === true && (stage.kind !== 'agent' || stage.output === undefined)) {
          context.addIssue({
            code: 'custom',
            message: `Only an agent stage with an output file can run in its own shell, and "${stage.id}" cannot`,
            path: at,
          })
        }
        const ownAgent = stage.agent !== undefined || stage.args !== undefined
        if (ownAgent && !canHaveOwnAgent(step, stage)) {
          context.addIssue({
            code: 'custom',
            message: `Only a stage outside Shell 1, or a custom stage that an agent works on, can have its own agent or arguments, and "${stage.id}" cannot`,
            path: at,
          })
        }
        if (stage.ci === true && !checksOnCi(stage)) {
          context.addIssue({
            code: 'custom',
            message: `Only a stage that changes code or runs tests can be checked on CI, and "${stage.id}" cannot`,
            path: named,
          })
        }
        if (stage.endShell === true && !canEndShell(stage)) {
          context.addIssue({
            code: 'custom',
            message: `Only a stage that starts an agent or a command can end its shell, and "${stage.id}" is a ${stage.kind} stage`,
            path: named,
          })
        }
        if (stage.workflow === true && stage.kind !== 'agent') {
          context.addIssue({
            code: 'custom',
            message: `Only agent stages can run with a workflow, and "${stage.id}" is a ${stage.kind} stage`,
            path: named,
          })
        }
        // The context comes first and the release last, so neither can be in a path.
        if (isBranch(step) && isOn(stage) && ['context', 'release'].includes(stage.kind)) {
          context.addIssue({
            code: 'custom',
            message: `"${stage.id}" is a ${stage.kind} stage, which cannot be in a path`,
            path: at,
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
      .filter(({ step }) => stagesOf(step).some(isOn))
    const [first] = on
    if (first === undefined) {
      context.addIssue({ code: 'custom', message: 'Turn on at least one stage' })
      return
    }
    // A branch needs the end of a stage before it to check its conditions.
    if (isBranch(first.step)) {
      context.addIssue({
        code: 'custom',
        message: 'A branch cannot be the first step',
        path: [first.index],
      })
    }
    // The event that skips a branch chooses no path for a branch right after it, so the conditions
    // of that one would never run. A branch left with only its Otherwise path is plain steps.
    const branchOn = ({ step }: (typeof on)[number]) => activeSteps([step]).some(isBranch)
    on.forEach((entry, at) => {
      const before = on[at - 1]
      if (before !== undefined && branchOn(before) && branchOn(entry)) {
        context.addIssue({
          code: 'custom',
          message: 'A branch cannot come right after a branch. Put a stage between them',
          path: [entry.index],
        })
      }
    })
    // The context is read when a task is made, so only the first step can be one. A release
    // ends the task, so nothing can come after it.
    const kindAt = ({ step }: (typeof on)[number]) => (isStage(step) ? step.kind : undefined)
    const contexts = on.filter((entry) => kindAt(entry) === 'context')
    const releases = on.filter((entry) => kindAt(entry) === 'release')
    if (contexts.length > 1) {
      context.addIssue({
        code: 'custom',
        message: 'A pipeline has at most one context stage',
        path: [contexts[1]?.index ?? 0],
      })
    } else if (contexts[0] !== undefined && contexts[0] !== first) {
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

// Review, manual and release stages start nothing in a shell, so they have none to end.
export function canEndShell(stage: Pick<Stage, 'kind'>): boolean {
  return stage.kind === 'context' || stage.kind === 'agent' || stage.kind === 'testing'
}

// CI checks code, so only a stage that changes it (an agent stage without an output file) or a
// testing stage can wait for CI.
export function checksOnCi(stage: Pick<Stage, 'kind' | 'output'>): boolean {
  return stage.kind === 'testing' || (stage.kind === 'agent' && stage.output === undefined)
}

// The last stage before stage `id` that changes code: where a CI failure of a later stage is fixed.
// With the task's `paths`, only the paths it took count.
export function codeStageBefore(
  steps: readonly Step[],
  id: string,
  paths?: Readonly<Record<string, string>>,
): Stage | undefined {
  return stagesBefore(steps, id, paths)
    .filter((stage) => stage.kind === 'agent' && stage.output === undefined)
    .at(-1)
}

// The stages of a step that a task went through: for a branch, the stages of the path it took.
// Without a record of the choices, every path counts.
export function takenStages(
  step: Step,
  paths: Readonly<Record<string, string>> | undefined,
): Stage[] {
  if (!isBranch(step) || paths === undefined) {
    return stagesOf(step)
  }
  return step.paths.find((path) => path.id === paths[step.id])?.stages ?? []
}

// The stages before stage `id` on the way the task went: the earlier steps, then the earlier
// stages of its own path.
function stagesBefore(
  steps: readonly Step[],
  id: string,
  paths: Readonly<Record<string, string>> | undefined,
): Stage[] {
  const index = Math.max(stepIndex(steps, id), 0)
  const place = pathOf(steps, id)
  return [
    ...steps.slice(0, index).flatMap((step) => takenStages(step, paths)),
    ...(place?.path.stages.slice(0, place.index) ?? []),
  ]
}

// The project settings let tasks check this stage on CI.
export function offersCi(stage: Pick<Stage, 'kind' | 'output' | 'ci'>): boolean {
  return checksOnCi(stage) && stage.ci === true
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
// Inside a branch the step is the current stage of the path.
export type StagePosition = {
  stage: string
  state: TaskState
  lanes: Record<string, TaskState>
  // The path the task took at each branch it reached: branch id -> path id, '' when it skipped
  // the branch.
  paths?: Record<string, string>
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

// A branch is entered at the first stage of `path`. Without a path of the branch there is nothing
// to enter: the caller moves past the branch instead.
export function enterStep(step: Step, state?: TaskState, path?: string): StagePosition {
  if (isBranch(step)) {
    const first = step.paths.find((each) => each.id === path)?.stages[0]
    return first === undefined
      ? { stage: step.id, state: state ?? 'ready', lanes: {} }
      : enterStep(first, state)
  }
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

// The step that runs for stage or step `id`: its group, or the stage itself, also inside a path.
export function stepToRun(steps: readonly Step[], id: string): Step | undefined {
  const step = findStep(steps, id)
  return step !== undefined && isBranch(step)
    ? (stagesOf(step).find((stage) => stage.id === id) ?? step)
    : step
}

// The branch and path of a stage in a path, and where the stage is in the path.
export function pathOf(
  steps: readonly Step[],
  stageId: string,
): { branch: StageBranch; path: BranchPath; index: number } | undefined {
  for (const step of steps) {
    if (!isBranch(step)) {
      continue
    }
    for (const path of step.paths) {
      const index = path.stages.findIndex((stage) => stage.id === stageId)
      if (index !== -1) {
        return { branch: step, path, index }
      }
    }
  }
  return undefined
}

// A stage of a branch where the task took another path, or none.
export function isNotTaken(
  steps: readonly Step[],
  paths: Readonly<Record<string, string>> | undefined,
  stageId: string,
): boolean {
  const place = pathOf(steps, stageId)
  const taken = place === undefined ? undefined : paths?.[place.branch.id]
  return taken !== undefined && taken !== place?.path.id
}

// The condition of a path in words, for the settings chart and the reason a task took it.
export function describeCondition(path: Pick<BranchPath, 'when' | 'not'>): string {
  const { when } = path
  if (when === undefined) {
    return 'Otherwise'
  }
  const not = path.not === true
  if (when.type === 'command') {
    return `${when.run} ${not ? 'does not succeed' : 'succeeds'}`
  }
  const where = when.section === undefined ? when.file : `${when.file} "${when.section}"`
  return `${where} ${not ? 'does not contain' : 'contains'} "${when.contains}"`
}

// The stage of a group that changes code keeps the first terminal (and the agent session of the
// steps before); each other stage gets the next terminal.
export function codeStage(step: Step): Stage {
  const stages = stagesOf(step)
  const agents = stages.filter((stage) => stage.kind === 'agent')
  return agents.find((stage) => stage.output === undefined) ?? agents[0] ?? (stages[0] as Stage)
}

// The shell of a stage that runs on its own apart from Shell 1. Shells 2 to 4 go to the stages of
// a group, so it does not take over the agent of one.
export const OWN_SHELL = '5'

// Each stage that starts its own agent or arguments gets a shell of its own, from Shell 6 in
// pipeline order. No shell ever holds two agents, so none is closed while the task runs.
const FIRST_STAGE_SHELL = 6

export function terminalOfStage(steps: readonly Step[], stageId: string): string {
  const own = steps.flatMap((step) => stagesOf(step).filter((stage) => runsOwnAgent(step, stage)))
  const index = own.findIndex((stage) => stage.id === stageId)
  if (index !== -1) {
    return String(FIRST_STAGE_SHELL + index)
  }
  const step = stepToRun(steps, stageId)
  if (step === undefined || isBranch(step)) {
    return '1'
  }
  if (!isGroup(step)) {
    return step.ownShell === true ? OWN_SHELL : '1'
  }
  const main = codeStage(step)
  if (main.id === stageId) {
    return '1'
  }
  const others = step.parallel.filter((stage) => stage.id !== main.id)
  return String(others.findIndex((stage) => stage.id === stageId) + 2)
}

// A stage that starts its own agent may use another one: an agent stage of a group that does not
// keep Shell 1 (that goes to the code stage of the stages that are on), or one in its own shell.
// A stage of your own that an agent works on may use another one too.
export function canHaveOwnAgent(
  step: Step,
  stage: Pick<Stage, 'id' | 'kind' | 'ownShell'>,
): boolean {
  if (isCustomAgentStage(stage)) {
    return true
  }
  if (stage.kind !== 'agent') {
    return false
  }
  if (!isGroup(step)) {
    return stage.ownShell === true
  }
  const on = step.parallel.filter(isOn)
  return codeStage({ ...step, parallel: on.length > 0 ? on : step.parallel }).id !== stage.id
}

function isCustomAgentStage(stage: Pick<Stage, 'id' | 'kind'>): boolean {
  return (
    catalogEntry(stage.id) === undefined && (stage.kind === 'agent' || stage.kind === 'testing')
  )
}

function runsOwnAgent(step: Step, stage: Stage): boolean {
  return (stage.agent !== undefined || stage.args !== undefined) && canHaveOwnAgent(step, stage)
}

// The agent and words of a stage that starts its own agent. A stage without them uses the task's
// agent.
export function stageAgent(step: Step, stage: Stage): Pick<Stage, 'agent' | 'args'> {
  if (!runsOwnAgent(step, stage)) {
    return {}
  }
  return {
    ...(stage.agent === undefined ? {} : { agent: stage.agent }),
    ...(stage.args === undefined ? {} : { args: stage.args }),
  }
}

export function stageOfTerminal(
  steps: readonly Step[],
  step: Step,
  terminal: string,
): Stage | undefined {
  return stagesOf(step).find((stage) => terminalOfStage(steps, stage.id) === terminal)
}

// The stage an agent terminal works or worked on, and the state of that stage. Both are null for a
// terminal that no stage up to now uses, such as an extra shell where the user started an agent.
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
  steps: readonly Step[],
  terminal: string,
): TerminalLane {
  const step = stepToRun(steps, position.stage)
  if (step === undefined) {
    return terminal === '1' ? { stage: position.stage, state: position.state } : NO_LANE
  }
  if (isStage(step)) {
    if (terminal === terminalOfStage(steps, step.id)) {
      return { stage: step.id, state: position.state }
    }
  } else if (isGroup(step)) {
    const stage = stageOfTerminal(steps, step, terminal)
    if (stage !== undefined) {
      return { stage: stage.id, state: position.lanes[stage.id] ?? 'done' }
    }
  }
  return earlierLane(steps, step, terminal, position.paths)
}

// A stage's shell stays open after its step, so it keeps the name of that stage. Shell 1 serves
// many stages: the last one before the current step names it. Paths not taken never ran.
function earlierLane(
  steps: readonly Step[],
  current: Step,
  terminal: string,
  paths: Readonly<Record<string, string>> | undefined,
): TerminalLane {
  const stage = stagesBefore(steps, current.id, paths)
    .filter((each) => terminalOfStage(steps, each.id) === terminal)
    .at(-1)
  return stage === undefined ? NO_LANE : { stage: stage.id, state: 'done' }
}

export const lanesSchema = z.record(stageIdSchema, taskStateSchema)
