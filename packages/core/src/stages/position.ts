import type { TaskState } from '../tasks/task-state.js'
import {
  type BranchPath,
  isBranch,
  isGroup,
  type Stage,
  type StageBranch,
  type Step,
  stagesOf,
} from './schema.js'

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
