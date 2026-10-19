import { homedir } from 'node:os'
import {
  completeEvent,
  entersBranch,
  isBranch,
  openSubtasks,
  RECORD_ONLY_EVENT_TYPES,
  type Stage,
  type Step,
  stageOfTerminal,
  stagesOf,
  stepToRun,
  type Task,
  type TaskEvent,
  type TaskState,
  type TerminalLane,
  terminalLane,
  waitingOn,
} from '@natamux/core'
import type { AgentRunner } from '../agents/agent-runner.js'
import { deleteRemoteRef } from '../git/commit-push.js'
import { type RunGh, runGh } from '../git/gh.js'
import type { DaemonMessage, EventBus } from '../shared/event-bus.js'
import { restoreAttachments } from '../tasks/attachments.js'
import type { ApplyEventResult } from '../tasks/task-inputs.js'
import type { TaskService } from '../tasks/task-service.js'
import type { OpenShells } from '../terminals/open-shells.js'
import { MAIN_TERMINAL } from '../terminals/terminal-limits.js'
import type { TerminalService } from '../terminals/terminal-service.js'
import { type CiResult, checkOnCi, ciBranchOf } from './ci-check.js'
import { StageAct } from './stage-act.js'
import {
  ENDS_STAGE,
  type EventInput,
  laneState,
  type StageAction,
  StageActionError,
  type StageActionInput,
} from './stage-actions.js'
import { type RunCommand, runShellCommand } from './stage-checks.js'
import { StageContext } from './stage-context.js'
import { archiveStageFiles, excludeStageDir, restoreStageFiles, workingDir } from './stage-files.js'
import { StageGate } from './stage-gate.js'
import { StageLauncher } from './stage-launcher.js'
import { pickPath } from './stage-paths.js'
import type { StageRun } from './stage-run.js'
import { StageShells } from './stage-shells.js'

type StageRunnerOptions = {
  tasks: TaskService
  terminals: TerminalService
  agents: AgentRunner
  bus: EventBus
  dataDir: string
  // The folder of a repository, for gh.
  repoPath: (repo: string | null) => string
  gh?: RunGh
  runCommand?: RunCommand
  now?: () => number
  // The platform of the terminals' shells.
  platform?: NodeJS.Platform
  // The home folder, where agents keep the user's skills.
  home?: string
  // Pushes the work of a task to its CI branch and waits for CI. Tests give a fake.
  checkCi?: (input: { worktree: string; branch: string }) => Promise<CiResult>
  // The shells that were open, to bring back after a restart.
  openShells?: OpenShells
}

// Moves tasks through their stages: starts each stage when the task reaches it, takes the report
// that a stage is done, checks the result and lets the gate decide (docs/design-task-stages.md).
export class StageRunner {
  private readonly run: StageRun
  private readonly bus: EventBus
  private readonly starting = new Set<string>()
  private readonly pending = new Set<Promise<unknown>>()
  // Finished tasks whose CI branch was deleted, so later updates do not push again.
  private readonly ciCleaned = new Set<string>()
  private unsubscribe: (() => void) | undefined

  constructor(options: StageRunnerOptions) {
    this.bus = options.bus
    // The parts call the runner's own steps through it, so a spy on the runner sees those calls.
    const run: StageRun = {
      tasks: options.tasks,
      terminals: options.terminals,
      agents: options.agents,
      dataDir: options.dataDir,
      repoPath: options.repoPath,
      gh: options.gh ?? runGh,
      runCommand: options.runCommand ?? runShellCommand,
      checkCi: options.checkCi ?? ((input) => checkOnCi(input)),
      now: options.now ?? Date.now,
      platform: options.platform ?? process.platform,
      home: options.home ?? homedir(),
      openShells: options.openShells,
      reported: new Map(),
      waitingForWorkspace: new Set(),
      waitingForTasks: new Set(),
      launched: new Map(),
      outputTails: new Map(),
      chosenAgents: new Map(),
      commandTerminals: new Map(),
      leaving: new Map(),
      startWork: new Map(),
      ciRounds: new Set(),
      checking: new Set(),
      get shells() {
        return shells
      },
      get launcher() {
        return launcher
      },
      get context() {
        return context
      },
      get actions() {
        return actions
      },
      get gate() {
        return gate
      },
      stepOf: (task) => this.stepOf(task),
      background: (work) => this.background(work),
      startLanes: (taskId, states, cause) => this.startLanes(taskId, states, cause),
      startStage: (taskId, step, stage, cause) => this.startStage(taskId, step, stage, cause),
      withPath: (taskId, input) => this.withPath(taskId, input),
      applyEnding: (taskId, input) => this.applyEnding(taskId, input),
    }
    const shells = new StageShells(run)
    const launcher = new StageLauncher(run)
    const context = new StageContext(run)
    const actions = new StageAct(run)
    const gate = new StageGate(run)
    this.run = run
  }

  start(): void {
    this.unsubscribe = this.bus.subscribe((message) => this.onMessage(message))
    this.run.gate.resumeCiChecks()
  }

  stop(): void {
    this.unsubscribe?.()
  }

  // Waits for the work the runner started in the background. For tests.
  async settle(): Promise<void> {
    while (this.pending.size > 0) {
      await Promise.allSettled([...this.pending])
    }
  }

  private background(work: Promise<unknown>): void {
    const tracked = work.catch(() => undefined).finally(() => this.pending.delete(tracked))
    this.pending.add(tracked)
  }

  private startInBackground(taskId: string, states: readonly TaskState[], cause?: TaskEvent) {
    const work = this.startLanes(taskId, states, cause).catch((error: unknown) => {
      console.error(`Could not start the stages of ${taskId}: ${String(error)}`)
    })
    this.run.startWork.set(taskId, work)
    this.background(
      work.finally(() => {
        if (this.run.startWork.get(taskId) === work) {
          this.run.startWork.delete(taskId)
        }
      }),
    )
  }

  // The step the task runs now. In a branch, that is the stage of the path it is at.
  private stepOf(task: Task): Step {
    const step = stepToRun(this.run.tasks.stepsOf(task), task.stage)
    if (step === undefined || isBranch(step)) {
      throw new StageActionError(`${task.id} is at an unknown stage "${task.stage}"`)
    }
    return step
  }

  // The stage a hook from this terminal is about, and its state. Null for a shell that no stage
  // uses, where the user started an agent by hand.
  laneFor(task: Task, terminal: string | undefined): { stage: string; state: TaskState } | null {
    const step = this.stepOf(task)
    const stage = stageOfTerminal(this.run.tasks.stepsOf(task), step, terminal ?? MAIN_TERMINAL)
    return stage === undefined ? null : { stage: stage.id, state: laneState(task, step, stage.id) }
  }

  // The stage an agent in this terminal works on, and its own state, for the dashboard.
  laneOfTerminal(task: Task, terminal: string): TerminalLane {
    return terminalLane(task, this.run.tasks.stepsOf(task), terminal)
  }

  // True once for the turn end that follows a report from this terminal.
  takeReport(taskId: string, terminal: string): boolean {
    return this.run.reported.delete(`${taskId}/${terminal}`)
  }

  private onMessage(message: DaemonMessage): void {
    if (message.kind === 'task.deleted') {
      this.run.shells.finish(message.taskId, { keepScreens: false })
      // A deleted subtask no longer holds its parent.
      this.startWaiting()
      return
    }
    if (message.kind === 'task.created') {
      this.onCreated(message.task)
      return
    }
    if (message.kind !== 'task.updated' || !message.accepted) {
      return
    }
    this.onUpdated(message.task, message.event, message.moved === true)
  }

  // A new task whose first step reads its context starts it at once.
  private onCreated(task: Task): void {
    const step = this.stepOf(task)
    if (stagesOf(step).some((stage) => stage.kind === 'context')) {
      this.startInBackground(task.id, ['draft', 'ready'])
    }
  }

  private onUpdated(task: Task, event: TaskEvent, moved: boolean): void {
    // Each piece of side work below has its own try, so a failed one does not stop the next stage.
    if (['stage.approved', 'stage.passed', 'stage.checked'].includes(event.type)) {
      this.archive(task)
    }
    this.deleteCiBranch(task)
    if (event.type === 'workspace.created' && task.workspace !== null) {
      this.onWorkspaceCreated(task, task.workspace)
    }
    if (ENDS_STAGE.includes(event.type)) {
      this.run.shells.endStageShell(task, event)
    }
    if (event.type === 'stage.reopened') {
      this.run.shells.endLeftShells(task)
    }
    if (moved) {
      this.startInBackground(task.id, ['ready'], event)
    } else if (event.type === 'agent.resumed' && event.source !== 'agent') {
      // After this message has reached every listener, so its events come in order.
      this.background(Promise.resolve().then(() => this.run.launcher.resumeStage(task.id, event)))
    }
    const ended = task.state === 'done' || task.state === 'cancelled'
    if (ended && !RECORD_ONLY_EVENT_TYPES.includes(event.type)) {
      this.run.shells.finish(task.id, { keepScreens: true })
    }
    // A cancelled subtask no longer holds its parent.
    if (ended || event.type === 'task.edited') {
      this.startWaiting()
    }
  }

  private archive(task: Task): void {
    try {
      archiveStageFiles(this.run.dataDir, task)
    } catch (error) {
      console.error(`Could not archive the stage files of ${task.id}: ${String(error)}`)
    }
  }

  private deleteCiBranch(task: Task): void {
    const finished = task.state === 'done' || task.state === 'cancelled'
    const usedCi = (task.ciStages ?? []).length > 0 && !this.ciCleaned.has(task.id)
    if (finished && usedCi && task.branch !== null) {
      this.ciCleaned.add(task.id)
      // The CI branch only held snapshots for checks. Gone already, or offline: only logged.
      const ref = ciBranchOf(task.branch)
      this.background(
        deleteRemoteRef(this.run.repoPath(task.repo), ref).catch((error: unknown) => {
          console.error(`Could not delete the CI branch ${ref} of ${task.id}: ${String(error)}`)
        }),
      )
    }
  }

  private onWorkspaceCreated(task: Task, workspace: string): void {
    try {
      restoreStageFiles(this.run.dataDir, task)
    } catch (error) {
      console.error(`Could not restore the stage files of ${task.id}: ${String(error)}`)
    }
    try {
      restoreAttachments(this.run.dataDir, task)
    } catch (error) {
      console.error(`Could not restore the attachments of ${task.id}: ${String(error)}`)
    }
    this.background(
      excludeStageDir(workspace).catch((error: unknown) => {
        console.error(`Could not ignore the stage folder of ${task.id}: ${String(error)}`)
      }),
    )
    if (this.run.waitingForWorkspace.delete(task.id)) {
      this.startInBackground(task.id, ['ready', 'draft'])
    }
  }

  // A task made with no agent: its stages that need one stay ready, and no agent starts in its
  // shell until a person starts one.
  leaveToPerson(taskId: string): void {
    this.run.tasks.setWithoutAgent(taskId, true)
  }

  // Starts the stages that waited for other tasks, once those are done.
  private startWaiting(): void {
    if (this.run.waitingForTasks.size === 0) {
      return
    }
    const all = this.run.tasks.listTasks()
    for (const id of [...this.run.waitingForTasks]) {
      const task = all.find((candidate) => candidate.id === id)
      if (
        task === undefined ||
        (waitingOn(task, all).length === 0 && openSubtasks(task, all).length === 0)
      ) {
        this.run.waitingForTasks.delete(id)
        if (task !== undefined) {
          this.startInBackground(id, ['ready', 'draft'])
        }
      }
    }
  }

  // Starts the stages of the task's step that are in one of `states`.
  async startLanes(taskId: string, states: readonly TaskState[], cause?: TaskEvent): Promise<void> {
    const forward = cause !== undefined && ENDS_STAGE.includes(cause.type)
    if (forward && this.markedLanes(taskId).length > 0) {
      // After the message of the move has reached every listener, so its events come in order.
      await Promise.resolve()
      if (await this.skipMarked(taskId)) {
        // The skips ended the step. The message of that move starts the next one.
        return
      }
    }
    const task = this.run.tasks.getTask(taskId)
    const step = this.stepOf(task)
    for (const stage of stagesOf(step)) {
      if (states.includes(laneState(task, step, stage.id))) {
        await this.startStage(task.id, step, stage, cause)
      }
    }
  }

  // The stages of the current step that the user marked to skip, and that are not done.
  private markedLanes(taskId: string): Stage[] {
    const task = this.run.tasks.getTask(taskId)
    const marked = task.skipStages ?? []
    const step = this.stepOf(task)
    return stagesOf(step).filter(
      (stage) => marked.includes(stage.id) && laneState(task, step, stage.id) !== 'done',
    )
  }

  // Skips the marked stages of the current step. True when that ended the step.
  private async skipMarked(taskId: string): Promise<boolean> {
    const before = this.run.tasks.getTask(taskId)
    for (const stage of this.markedLanes(taskId)) {
      await this.applyEnding(taskId, {
        source: 'system',
        type: 'stage.skipped',
        payload: { stage: stage.id, planned: true },
      })
    }
    const after = this.run.tasks.getTask(taskId)
    return after.stage !== before.stage || after.state === 'done'
  }

  private async startStage(taskId: string, step: Step, stage: Stage, cause?: TaskEvent) {
    const key = `${taskId}/${stage.id}`
    if (this.starting.has(key)) {
      return
    }
    this.starting.add(key)
    try {
      const task = this.run.tasks.getTask(taskId)
      const { context, launcher } = this.run
      switch (stage.kind) {
        case 'context':
          await context.startContext(task, step, stage)
          return
        case 'agent': {
          const resumed = cause?.type === 'agent.resumed'
          const promptFor = launcher.promptOf(task, stage, cause)
          await launcher.startAgent(task, step, stage, promptFor, resumed)
          return
        }
        case 'testing':
          if (stage.command === undefined) {
            await launcher.startAgent(task, step, stage, launcher.promptOf(task, stage))
          } else {
            await launcher.startCommand(task, stage, stage.command)
          }
          return
        default:
          // Review, manual and release stages wait for a person or a merge.
          return
      }
    } finally {
      this.starting.delete(key)
    }
  }

  resumeAgents(): void {
    this.run.launcher.resumeAgents()
  }

  loadOpenShells(): void {
    this.run.shells.loadOpenShells()
  }

  restoreTask(taskId: string, attached: string): void {
    this.run.shells.restoreTask(taskId, attached)
  }

  startByHand(
    taskId: string,
    agent: string,
    prompt?: string,
    makeWorkspace?: () => Promise<unknown>,
  ): Promise<Task> {
    return this.run.launcher.startByHand(taskId, agent, prompt, makeWorkspace)
  }

  agentRuns(taskId: string, terminal = MAIN_TERMINAL): boolean {
    return this.run.shells.agentRuns(taskId, terminal)
  }

  agentIn(taskId: string, terminal: string): Promise<string | null> {
    return this.run.shells.agentIn(taskId, terminal)
  }

  onOutput(taskId: string, terminal: string, data: Buffer): void {
    this.run.shells.onOutput(taskId, terminal, data)
  }

  act(taskId: string, action: StageAction, input: StageActionInput = {}): Promise<Task> {
    return this.run.actions.act(taskId, action, input)
  }

  // A move into a branch takes the first path whose condition holds. The conditions are checked
  // here, before the move, and the event keeps the choice, so a replay of the log never runs them.
  async withPath(taskId: string, input: EventInput): Promise<EventInput> {
    const task = this.run.tasks.getTask(taskId)
    const steps = this.run.tasks.stepsOf(task)
    const position = { stage: task.stage, state: task.state, lanes: task.lanes, paths: task.paths }
    const at = new Date(this.run.now()).toISOString()
    const event = completeEvent(position, { ...input, taskId, at }, steps)
    const branch = entersBranch(position, event, steps)
    if (branch === undefined) {
      const { path: _path, because: _because, ...payload } = input.payload
      return { ...input, payload }
    }
    // The conditions and their order come from the settings now, as names and gates do. A path
    // whose stages are all off now still runs for a task that has it, so it is checked too.
    const current = this.run.tasks
      .settingsOf(task.repo)
      .stages.find((step) => step.id === branch.id)
    const folder = workingDir(this.run.dataDir, task)
    const picked = await pickPath(
      current !== undefined && isBranch(current) ? current : branch,
      { folder, cwd: task.workspace ?? folder, paths: branch.paths.map((path) => path.id) },
      this.run.runCommand,
    )
    // Only the daemon picks the path. The stage stays the one the event was for, though the
    // conditions took a while.
    const { path: _path, because: _because, ...payload } = event.payload
    const path = picked.path === undefined ? {} : { path: picked.path }
    return { ...input, payload: { ...payload, ...path, because: picked.because } }
  }

  // An event that may end a stage, with the path it takes at a branch it enters.
  private async applyEnding(taskId: string, input: EventInput): Promise<ApplyEventResult> {
    return this.run.tasks.applyEvent(taskId, await this.withPath(taskId, input))
  }
}
