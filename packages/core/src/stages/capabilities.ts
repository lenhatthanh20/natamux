import { pathOf, stepIndex } from './position.js'
import { isBranch, type Stage, type Step, stagesOf } from './schema.js'
import { catalogEntry } from './stage-catalog.js'

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
export function stagesBefore(
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
