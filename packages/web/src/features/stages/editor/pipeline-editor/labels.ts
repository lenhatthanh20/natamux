import {
  activeSteps,
  type BranchPath,
  describeCondition,
  isBranch,
  isGroup,
  type Stage,
  type StageBranch,
  type StageGroup,
  type StageKind,
  type Step,
  stageName,
} from '@natamux/core'
import { alwaysWaits, takesOwnAgent } from './stage-fields'

export const KIND_LABEL: Record<StageKind, string> = {
  context: 'Context',
  agent: 'Agent',
  review: 'Code review',
  testing: 'Testing',
  release: 'Release',
  manual: 'Check by a person',
}

export function approvalLabel(stage: Pick<Stage, 'kind' | 'gate'>): string {
  if (alwaysWaits(stage)) {
    return 'Always waits for you'
  }
  if (stage.kind === 'release') {
    return stage.gate === 'manual'
      ? 'Opens the pull request when you click'
      : 'Opens the pull request by itself'
  }
  return stage.gate === 'manual' ? 'Waits for your approval' : 'Moves on by itself'
}

// The model in the words of an agent's command line: `--model x`, `--model=x` or `-m x`.
export function modelOf(args: string | undefined): string | undefined {
  const words = args?.trim().split(/\s+/) ?? []
  for (const [index, word] of words.entries()) {
    if (word.startsWith('--model=')) {
      return word.slice('--model='.length) || undefined
    }
    if (word === '--model' || word === '-m') {
      const next = words[index + 1]
      return next === undefined || next.startsWith('-') ? undefined : next
    }
  }
  return undefined
}

// The agent and model of a stage that starts its own agent. A null agent is the task's. Nothing
// for a stage that uses the task's agent, or whose agent and args are not used.
export type StageRunner = { agent: string | null; model?: string }

export function stageRunner(steps: readonly Step[], stage: Stage): StageRunner | undefined {
  if ((stage.agent === undefined && stage.args === undefined) || !takesOwnAgent(steps, stage.id)) {
    return undefined
  }
  const model = modelOf(stage.args)
  return { agent: stage.agent ?? null, ...(model === undefined ? {} : { model }) }
}

// "A, B and C"
export function joinNames(names: readonly string[]): string {
  if (names.length <= 1) {
    return names[0] ?? ''
  }
  return `${names.slice(0, -1).join(', ')} and ${names.at(-1)}`
}

export function groupName(group: StageGroup): string {
  return group.name ?? joinNames(group.parallel.map(stageName))
}

export function branchName(branch: Pick<StageBranch, 'id' | 'name'>): string {
  return stageName(branch)
}

export function pathName(path: Pick<BranchPath, 'id' | 'name'>): string {
  return stageName(path)
}

// The name of a step in a sentence: a stage, a group or a branch.
export function stepName(step: Step): string {
  if (isGroup(step)) {
    return groupName(step)
  }
  return isBranch(step) ? branchName(step) : stageName(step)
}

// "Small change if plan.md contains "small"", or "Review PR, otherwise".
export function pathLabel(path: BranchPath): string {
  return path.when === undefined
    ? `${pathName(path)}, otherwise`
    : `${pathName(path)} if ${describeCondition(path)}`
}

// "After plan, one path of 2: Small change if …; Review PR, otherwise"
export function branchLabel(branch: StageBranch): string {
  return `${branchName(branch)}, one path of ${branch.paths.length}: ${branch.paths.map(pathLabel).join('; ')}`
}

// What the preview says to a screen reader: the stages that are on, in order.
export function pipelineLabel(steps: readonly Step[]): string {
  const parts = activeSteps(steps).map((step) => {
    if (isBranch(step)) {
      return branchLabel(step)
    }
    return isGroup(step)
      ? `${joinNames(step.parallel.map(stageName))} side by side`
      : stageName(step)
  })
  return parts.length === 0 ? 'Pipeline: no stage is on' : `Pipeline: ${parts.join(', then ')}`
}
