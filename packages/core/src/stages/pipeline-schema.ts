import { z } from 'zod'
import { canEndShell, checksOnCi } from './capabilities.js'
import {
  activeSteps,
  isBranch,
  isGroup,
  isOn,
  isStage,
  type Stage,
  type Step,
  stagesOf,
  stepSchema,
} from './schema.js'
import { canHaveOwnAgent } from './terminal-lane.js'

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

type Path = (string | number)[]
type Unique = (id: string, message: string, path: Path) => void
type Entry = { step: Step; index: number }

function fail(context: z.RefinementCtx, message: string, path?: Path) {
  context.addIssue({ code: 'custom', message, ...(path === undefined ? {} : { path }) })
}

function checkBranchPaths(step: Step, index: number, unique: Unique, context: z.RefinementCtx) {
  if (!isBranch(step)) {
    return
  }
  const last = step.paths.length - 1
  step.paths.forEach((path, p) => {
    unique(path.id, `The path id "${path.id}" is used twice`, [index, 'paths', p])
    if (path.when === undefined && p !== last) {
      fail(context, 'Only the last path can be the Otherwise path', [index, 'paths', p])
    }
  })
  // A path with all stages off is left out of new tasks. Only an Otherwise path can be the
  // one left, since its stages then run as plain steps.
  const filled = step.paths.filter((path) => path.stages.length > 0)
  const left = filled.filter((path) => path.stages.some(isOn))
  if (filled.length >= 2 && left.length === 1 && left[0]?.when !== undefined) {
    fail(context, 'Turn on a stage in another path, or remove the branch', [index])
  }
}

function checkStageRules(step: Step, index: number, unique: Unique, context: z.RefinementCtx) {
  for (const { stage, at } of placesOf(step, index)) {
    // Outside a branch these rules name the whole step, as they did before branches.
    const named = isBranch(step) ? at : [index]
    if (isGroup(step) || isBranch(step)) {
      unique(stage.id, `The stage id "${stage.id}" is used twice`, named)
    }
    if (stage.ownShell === true && (stage.kind !== 'agent' || stage.output === undefined)) {
      fail(
        context,
        `Only an agent stage with an output file can run in its own terminal, and "${stage.id}" cannot`,
        at,
      )
    }
    const ownAgent = stage.agent !== undefined || stage.args !== undefined
    if (ownAgent && !canHaveOwnAgent(step, stage)) {
      fail(
        context,
        `Only a stage outside Terminal 1, or a custom stage that an agent works on, can have its own agent or arguments, and "${stage.id}" cannot`,
        at,
      )
    }
    if (stage.ci === true && !checksOnCi(stage)) {
      fail(
        context,
        `Only a stage that changes code or runs tests can be checked on CI, and "${stage.id}" cannot`,
        named,
      )
    }
    if (stage.endShell === true && !canEndShell(stage)) {
      fail(
        context,
        `Only a stage that starts an agent or a command can end its terminal, and "${stage.id}" is a ${stage.kind} stage`,
        named,
      )
    }
    if (stage.workflow === true && stage.kind !== 'agent') {
      fail(
        context,
        `Only agent stages can run with a workflow, and "${stage.id}" is a ${stage.kind} stage`,
        named,
      )
    }
    // The context comes first and the release last, so neither can be in a path.
    if (isBranch(step) && isOn(stage) && ['context', 'release'].includes(stage.kind)) {
      fail(context, `"${stage.id}" is a ${stage.kind} stage, which cannot be in a path`, at)
    }
  }
}

function checkStepRules(steps: readonly Step[], context: z.RefinementCtx) {
  const seen = new Set<string>()
  const unique: Unique = (id, message, path) => {
    if (seen.has(id)) {
      fail(context, message, path)
    }
    seen.add(id)
  }
  steps.forEach((step, index) => {
    unique(step.id, `The stage id "${step.id}" is used twice`, [index])
    checkBranchPaths(step, index, unique, context)
    checkStageRules(step, index, unique, context)
    if (isGroup(step) && step.parallel.filter(isOn).length === 1) {
      fail(context, 'A group needs 2 stages that are on. Turn on another, or run it on its own', [
        index,
      ])
    }
  })
}

// The event that skips a branch chooses no path for a branch right after it, so the conditions
// of that one would never run. A branch left with only its Otherwise path is plain steps.
function checkBranchOrder(on: Entry[], context: z.RefinementCtx) {
  const [first] = on
  // A branch needs the end of a stage before it to check its conditions.
  if (first !== undefined && isBranch(first.step)) {
    fail(context, 'A branch cannot be the first step', [first.index])
  }
  const branchOn = ({ step }: Entry) => activeSteps([step]).some(isBranch)
  on.forEach((entry, at) => {
    const before = on[at - 1]
    if (before !== undefined && branchOn(before) && branchOn(entry)) {
      fail(context, 'A branch cannot come right after a branch. Put a stage between them', [
        entry.index,
      ])
    }
  })
}

// The context is read when a task is made, so only the first step can be one. A release
// ends the task, so nothing can come after it.
function checkContextAndRelease(on: Entry[], context: z.RefinementCtx) {
  const kindAt = ({ step }: Entry) => (isStage(step) ? step.kind : undefined)
  const contexts = on.filter((entry) => kindAt(entry) === 'context')
  const releases = on.filter((entry) => kindAt(entry) === 'release')
  if (contexts.length > 1) {
    fail(context, 'A pipeline has at most one context stage', [contexts[1]?.index ?? 0])
  } else if (contexts[0] !== undefined && contexts[0] !== on[0]) {
    fail(context, 'The context stage must come first', [contexts[0].index])
  }
  if (releases.length > 1) {
    fail(context, 'A pipeline has at most one release stage', [releases[1]?.index ?? 0])
  } else if (releases[0] !== undefined && releases[0] !== on.at(-1)) {
    fail(context, 'The release stage must come last', [releases[0].index])
  }
}

export const pipelineSchema = z
  .array(stepSchema)
  .min(1, 'A pipeline has at least one stage')
  .superRefine((steps, context) => {
    checkStepRules(steps, context)
    // Only stages that are on count from here: the others are not in new tasks.
    const on = steps
      .map((step, index) => ({ step, index }))
      .filter(({ step }) => stagesOf(step).some(isOn))
    if (on.length === 0) {
      fail(context, 'Turn on at least one stage')
      return
    }
    checkBranchOrder(on, context)
    checkContextAndRelease(on, context)
  })
