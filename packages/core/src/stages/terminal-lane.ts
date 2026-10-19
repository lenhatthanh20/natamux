import { z } from 'zod'
import { taskStateSchema } from '../tasks/task-state.js'
import { stagesBefore } from './capabilities.js'
import { codeStage, type StagePosition, stepToRun } from './position.js'
import {
  isBranch,
  isGroup,
  isOn,
  isStage,
  type Stage,
  type Step,
  stageIdSchema,
  stagesOf,
} from './schema.js'
import { catalogEntry } from './stage-catalog.js'

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

// Shell 1 keeps the task's agent and its session, so endShell never closes it. A testing stage
// with a command runs it in a shell of its own, which it may end.
export function endsShell1(
  steps: readonly Step[],
  stage: Pick<Stage, 'id' | 'kind' | 'command'>,
): boolean {
  if (stage.kind === 'testing' && stage.command !== undefined) {
    return false
  }
  return terminalOfStage(steps, stage.id) === '1'
}

export const lanesSchema = z.record(stageIdSchema, taskStateSchema)
