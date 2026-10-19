import type { Stage, Step, Task, TaskEvent, TaskState } from '@natamux/core'
import type { AgentRunner } from '../agents/agent-runner.js'
import type { SkillContext } from '../agents/agent-skills.js'
import type { RunGh } from '../git/gh.js'
import type { ApplyEventResult } from '../tasks/task-inputs.js'
import type { TaskService } from '../tasks/task-service.js'
import type { OpenShells } from '../terminals/open-shells.js'
import type { TerminalService } from '../terminals/terminal-service.js'
import type { CiResult } from './ci-check.js'
import type { EventInput, StageAction, StageActionInput } from './stage-actions.js'
import type { RunCommand } from './stage-checks.js'

export type PromptFor = (agent: string) => { text: string; missing: string[]; workflow?: boolean }

// What the parts call on each other, written out here so this file imports none of them.
type ShellsPart = {
  agentIn(taskId: string, terminal: string): Promise<string | null>
  agentRuns(taskId: string, terminal?: string): boolean
  endLeftShells(task: Task): void
  endStageShell(task: Task, event: TaskEvent): void
  finish(taskId: string, options: { keepScreens: boolean }): void
  loadOpenShells(): void
  onOutput(taskId: string, terminal: string, data: Buffer): void
  restoreTask(taskId: string, attached: string): void
}

type LauncherPart = {
  failStage(taskId: string, stage: string, reason: string): void
  markStarted(task: Task, stage: Stage, payload: Record<string, unknown>): boolean
  promptOf(task: Task, stage: Stage, cause?: TaskEvent): PromptFor
  resumeAgents(): void
  resumeStage(taskId: string, event: TaskEvent): void
  skillContext(task: Task, agent: string): SkillContext
  startAgent(
    task: Task,
    step: Step,
    stage: Stage,
    promptFor: PromptFor,
    resumed?: boolean,
  ): Promise<void>
  startByHand(
    taskId: string,
    agent: string,
    prompt?: string,
    makeWorkspace?: () => Promise<unknown>,
  ): Promise<Task>
  startCommand(task: Task, stage: Stage, command: string): Promise<void>
}

type ContextPart = {
  isAgentContext(task: Task): boolean
  startContext(task: Task, step: Step, stage: Stage): Promise<void>
}

type ActionsPart = {
  act(taskId: string, action: StageAction, input?: StageActionInput): Promise<Task>
  sendBacksFrom(taskId: string, from: string): number
}

type GatePart = {
  reportAndCheck(
    taskId: string,
    stage: string,
    source: TaskEvent['source'],
    terminal?: string,
  ): Promise<void>
  resumeCiChecks(): void
}

// What the parts of the runner share: its options, its state, the parts and the runner's own
// steps. One object, so no part keeps a copy of the state.
export type StageRun = {
  readonly tasks: TaskService
  readonly terminals: TerminalService
  readonly agents: AgentRunner
  readonly dataDir: string
  readonly repoPath: (repo: string | null) => string
  readonly gh: RunGh
  readonly runCommand: RunCommand
  readonly now: () => number
  readonly platform: NodeJS.Platform
  readonly home: string
  readonly checkCi: (input: { worktree: string; branch: string }) => Promise<CiResult>
  readonly openShells: OpenShells | undefined
  // Terminals whose agent reported a stage done, with that stage: the turn end that follows the
  // report is not a "stopped early". Kept by terminal, since the task may be at the next stage
  // when the turn ends.
  readonly reported: Map<string, string>
  // Tasks whose agent stage waits for a workspace.
  readonly waitingForWorkspace: Set<string>
  // Tasks whose agent stage waits for the tasks it depends on to be done.
  readonly waitingForTasks: Set<string>
  // Terminals where this daemon started an agent: which one, and when.
  readonly launched: Map<string, { agent: string; at: number }>
  // The end of the output of those terminals, on Windows.
  readonly outputTails: Map<string, string>
  // The agent a person picked, while the stage it starts is starting.
  readonly chosenAgents: Map<string, string>
  // The terminal where each task runs its stage command.
  readonly commandTerminals: Map<string, string>
  // The step each task leaves by the send back it is sending now. The event does not say it.
  readonly leaving: Map<string, Step>
  // The stages each task is starting now, so an action can answer with the task after them.
  readonly startWork: Map<string, Promise<void>>
  // The CI checks that run now, by task and stage. A second report waits for the first check.
  readonly ciRounds: Set<string>
  // The local checks that run now, by task, stage and round. Two reports at once (the agent's
  // and a person's) would run them twice in one worktree, and the second failure would count as
  // the agent's last try.
  readonly checking: Set<string>
  readonly shells: ShellsPart
  readonly launcher: LauncherPart
  readonly context: ContextPart
  readonly actions: ActionsPart
  readonly gate: GatePart
  stepOf(task: Task): Step
  background(work: Promise<unknown>): void
  startLanes(taskId: string, states: readonly TaskState[], cause?: TaskEvent): Promise<void>
  startStage(taskId: string, step: Step, stage: Stage, cause?: TaskEvent): Promise<void>
  withPath(taskId: string, input: EventInput): Promise<EventInput>
  applyEnding(taskId: string, input: EventInput): Promise<ApplyEventResult>
}
