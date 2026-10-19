import { pipelineSchema } from './pipeline-schema.js'
import { isBranch, isGroup, isOn, type Step, stageSchema, stagesOf } from './schema.js'
import { catalogEntry } from './stage-catalog.js'

// A problem of a pipeline that the settings screen shows next to its stage. Errors stop a save;
// warnings do not.
export type PipelineIssue = {
  // The stage or group the problem is about, or null for the whole pipeline.
  stage: string | null
  message: string
  level: 'error' | 'warning'
}

const FIELD_LABELS: Record<string, string> = {
  id: 'Id',
  name: 'Name',
  prompt: 'Prompt',
  output: 'Output file',
  sections: 'Required sections',
  command: 'Command',
  check: 'Check command',
  doneWhen: 'Finished when',
  agent: 'Agent',
  args: 'Arguments',
}

// The rules of `pipelineSchema` and of the catalog, as one message per problem. Each stage is
// checked on its own first, so a wrong field names its stage instead of the whole step.
export function pipelineIssues(steps: readonly Step[]): PipelineIssue[] {
  const issues: PipelineIssue[] = []
  for (const stage of steps.flatMap(stagesOf)) {
    // A path holds single stages only. The schema only says such a branch is not valid.
    if ('parallel' in stage || 'paths' in stage) {
      const branch = steps.find((step) => stagesOf(step).includes(stage))
      issues.push({
        stage: branch?.id ?? null,
        message: 'A path holds single stages only, not a group or a branch',
        level: 'error',
      })
      continue
    }
    const parsed = stageSchema.safeParse(stage)
    for (const issue of parsed.error?.issues ?? []) {
      const field = String(issue.path[0] ?? '')
      issues.push({
        stage: stage.id,
        message: `${FIELD_LABELS[field] ?? field}: ${issue.message}`,
        level: 'error',
      })
    }
    const needsCommand = stage.kind === 'testing' && catalogEntry(stage.id)?.needsCommand === true
    if (isOn(stage) && needsCommand && !stage.command) {
      issues.push({ stage: stage.id, message: 'Give the command to run', level: 'error' })
    }
  }
  if (issues.length === 0) {
    const parsed = pipelineSchema.safeParse(steps)
    for (const issue of parsed.error?.issues ?? []) {
      issues.push({ stage: stageAt(steps, issue.path), message: issue.message, level: 'error' })
    }
  }
  for (const step of steps) {
    if (!isGroup(step)) {
      continue
    }
    const writers = step.parallel.filter(
      (stage) => isOn(stage) && stage.kind === 'agent' && stage.output === undefined,
    )
    if (writers.length > 1) {
      issues.push({
        stage: step.id,
        message: `${writers.length} stages without an output file change code in one worktree at the same time`,
        level: 'warning',
      })
    }
  }
  return issues
}

function stageAt(steps: readonly Step[], path: readonly PropertyKey[]): string | null {
  const [index, field, lane, , stage] = path
  const step = typeof index === 'number' ? steps[index] : undefined
  if (step === undefined) {
    return null
  }
  if (isGroup(step) && field === 'parallel' && typeof lane === 'number') {
    return step.parallel[lane]?.id ?? step.id
  }
  // A stage in a path, such as [2, 'paths', 0, 'stages', 1]; else the branch itself.
  if (isBranch(step) && typeof lane === 'number' && typeof stage === 'number') {
    return step.paths[lane]?.stages[stage]?.id ?? step.id
  }
  return step.id
}
