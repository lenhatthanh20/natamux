import { z } from 'zod'

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

// Not .max(4): since zod 4.6.6 a failed max aborts, and the step union then says only
// "Invalid input" instead of our message.
const upTo4 = (list: readonly unknown[]) => list.length <= 4

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
  // next stage in that shell then starts a new agent session. Shell 1 is never closed: a stage
  // there keeps it, so files that set it still load.
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
    .refine(upTo4, 'A group holds 2 to 4 stages'),
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
    .refine(upTo4, 'A branch holds 2 to 4 paths'),
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
