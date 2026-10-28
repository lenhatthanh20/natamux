import {
  canHaveOwnAgent,
  catalogEntry,
  commitRule,
  defaultPrompt,
  type Gate,
  isBranch,
  isGroup,
  type Stage,
  type Step,
  stageName,
  stagesOf,
  terminalOfStage,
} from '@natamux/core'
import { findStage, groupOf, stepOfStage } from './find'

function updateStage(steps: readonly Step[], id: string, change: (stage: Stage) => Stage): Step[] {
  return steps.map((step) => {
    if (isBranch(step)) {
      return step.paths.some((path) => path.stages.some((stage) => stage.id === id))
        ? {
            ...step,
            paths: step.paths.map((path) => ({
              ...path,
              stages: path.stages.map((stage) => (stage.id === id ? change(stage) : stage)),
            })),
          }
        : step
    }
    if (isGroup(step)) {
      return step.parallel.some((stage) => stage.id === id)
        ? {
            ...step,
            parallel: step.parallel.map((stage) => (stage.id === id ? change(stage) : stage)),
          }
        : step
    }
    return step.id === id ? change(step) : step
  })
}

// Sets a field, or removes it when the value is left out.
function withField<K extends keyof Stage>(
  stage: Stage,
  key: K,
  value: Stage[K] | undefined,
): Stage {
  const { [key]: _, ...rest } = stage
  return (value === undefined ? rest : { ...rest, [key]: value }) as Stage
}

// A stage that is off stays in the file with its settings; new tasks do not get it.
export function setEnabled(steps: readonly Step[], id: string, on: boolean): Step[] {
  return updateStage(steps, id, (stage) => withField(stage, 'enabled', on ? undefined : false))
}

export function setGate(steps: readonly Step[], id: string, gate: Gate): Step[] {
  return updateStage(steps, id, (stage) => ({ ...stage, gate }))
}

// Tasks get a "Run with a workflow" checkbox on the stage. Off leaves the field out.
export function setWorkflow(steps: readonly Step[], id: string, on: boolean): Step[] {
  return updateStage(steps, id, (stage) => withField(stage, 'workflow', on ? true : undefined))
}

export function setCi(steps: readonly Step[], id: string, on: boolean): Step[] {
  return updateStage(steps, id, (stage) => withField(stage, 'ci', on ? true : undefined))
}

// Off leaves the field out, so a stage of the catalog stays as the catalog has it.
export function setEndShell(steps: readonly Step[], id: string, on: boolean): Step[] {
  return updateStage(steps, id, (stage) => withField(stage, 'endShell', on ? true : undefined))
}

// Off keeps the stage's own agent: the stage then runs in a shell of its own from Shell 6.
export function setOwnShell(steps: readonly Step[], id: string, on: boolean): Step[] {
  return updateStage(steps, id, (stage) => withField(stage, 'ownShell', on ? true : undefined))
}

// The catalog runs its checks (Test spec, Review and the like) in a terminal of their own.
export function defaultOwnShell(stage: Pick<Stage, 'id'>): boolean {
  return catalogEntry(stage.id)?.stage.ownShell === true
}

// Only an agent stage with an output may run in its own terminal; a group ignores it. A stage
// that has it anyway shows it, so it can be cleared.
export function offersOwnShell(steps: readonly Step[], id: string): boolean {
  const stage = findStage(steps, id)
  if (stage === undefined) {
    return false
  }
  const fits = stage.kind === 'agent' && stage.output !== undefined
  return stage.ownShell === true || (fits && groupOf(steps, id) === undefined)
}

export function runsInOwnShell(steps: readonly Step[], id: string): boolean {
  return findStage(steps, id)?.ownShell === true && groupOf(steps, id) === undefined
}

// The catalog offers CI on Implement and Automated tests; a stage of your own does not.
export function defaultCi(stage: Pick<Stage, 'id'>): boolean {
  return catalogEntry(stage.id)?.stage.ci === true
}

// The catalog offers a workflow on its hard stages; a stage of your own does not.
export function defaultWorkflow(stage: Pick<Stage, 'id' | 'kind'>): boolean {
  return stage.kind === 'agent' && catalogEntry(stage.id)?.stage.workflow === true
}

export type TextField =
  | 'name'
  | 'prompt'
  | 'command'
  | 'check'
  | 'output'
  | 'doneWhen'
  | 'agent'
  | 'args'

// `run` is the command of a path's condition; its `stage` is the path.
export type ShellField = {
  stage: Pick<Stage, 'id' | 'name'>
  field: 'check' | 'command' | 'args' | 'run'
  value: string
}

function stageFields(stage: Stage): ShellField[] {
  return (['check', 'command', 'args'] as const).flatMap((field) => {
    const value = stage[field]
    return value === undefined ? [] : [{ stage, field, value }]
  })
}

// What a settings file would run in a shell or put on an agent's command line, so an import can
// show it before it lands. Stages that are off count: they can be turned on later.
export function shellFields(steps: readonly Step[]): ShellField[] {
  return steps.flatMap((step) =>
    isBranch(step)
      ? step.paths.flatMap((path): ShellField[] => [
          ...(path.when?.type === 'command'
            ? [{ stage: path, field: 'run' as const, value: path.when.run }]
            : []),
          ...path.stages.flatMap(stageFields),
        ])
      : stagesOf(step).flatMap(stageFields),
  )
}

// Every agent or testing stage may pick another agent. One that would run in Shell 1 then runs in
// a shell of its own.
export function takesOwnAgent(steps: readonly Step[], id: string): boolean {
  const stage = findStage(steps, id)
  return stage !== undefined && canHaveOwnAgent(stage)
}

export function runsInShellOne(steps: readonly Step[], id: string): boolean {
  return stepOfStage(steps, id) !== undefined && terminalOfStage(steps, id) === '1'
}

// The default prompt as the agent gets it: the daemon puts the commit rule in `{commit}` of a
// default prompt only (see stage-prompts.ts), so a prompt that starts from this one keeps the rule.
export function shownDefaultPrompt(
  stage: Pick<Stage, 'id' | 'kind' | 'name' | 'output' | 'requireCommit'>,
): string {
  return defaultPrompt(stage).replaceAll('{commit}', commitRule(stage))
}

// An empty value removes the field. A name equal to the default name and the default prompt are
// left out too, so the file keeps only what differs.
export function setText(
  steps: readonly Step[],
  id: string,
  field: TextField,
  value: string,
): Step[] {
  return updateStage(steps, id, (stage) => {
    const empty = value.trim() === ''
    const isDefault =
      (field === 'name' && value === stageName({ id: stage.id })) ||
      (field === 'prompt' &&
        (value === defaultPrompt(stage) || value === shownDefaultPrompt(stage)))
    return withField(stage, field, empty || isDefault ? undefined : value)
  })
}

export function setSections(
  steps: readonly Step[],
  id: string,
  sections: readonly string[],
): Step[] {
  const kept = sections.map((section) => section.trim()).filter((section) => section !== '')
  return updateStage(steps, id, (stage) =>
    withField(stage, 'sections', kept.length === 0 ? undefined : kept),
  )
}

// Review and manual stages always wait for a person.
export function alwaysWaits(stage: Pick<Stage, 'kind'>): boolean {
  return stage.kind === 'review' || stage.kind === 'manual'
}

// The catalog's gate; a stage of your own waits for your approval.
export function defaultGate(stage: Pick<Stage, 'id' | 'kind'>): Gate {
  if (alwaysWaits(stage)) {
    return 'manual'
  }
  return catalogEntry(stage.id)?.stage.gate ?? 'manual'
}

export function isDefaultGate(stage: Pick<Stage, 'id' | 'kind' | 'gate'>): boolean {
  return stage.gate === defaultGate(stage)
}

export function hasCustomPrompt(stage: Pick<Stage, 'prompt'>): boolean {
  return stage.prompt !== undefined
}

// Agent and testing stages get a prompt.
export function takesPrompt(stage: Pick<Stage, 'kind'>): boolean {
  return stage.kind === 'agent' || stage.kind === 'testing'
}
