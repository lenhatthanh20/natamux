import { mkdirSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import {
  findStep,
  isGroup,
  runsWorkflow,
  type Stage,
  type Step,
  stageAgent,
  stagesOf,
  type Task,
  type TaskEvent,
  terminalOfStage,
  waitingOn,
} from '@natamux/core'
import { isLaunchable, LAST_SESSION } from '../agents/agent-launch.js'
import { quoteArgument } from '../agents/agent-runner.js'
import { fillSkills, type SkillContext } from '../agents/agent-skills.js'
import { MAIN_TERMINAL } from '../terminals/terminal-limits.js'
import {
  KEEPS_TURN,
  laneState,
  RESUMED_STATES,
  runByAgent,
  sendBackReason,
} from './stage-actions.js'
import { currentHead } from './stage-checks.js'
import { excludeStageDir } from './stage-files.js'
import { stagesWaitingForCi } from './stage-gate.js'
import { stagePrompt } from './stage-prompts.js'
import type { PromptFor, StageRun } from './stage-run.js'

// What Resume types to an agent that still runs and ended its turn.
export const GO_ON_PROMPT = 'Go on with the stage you are working on.'

// A stage prompt for the agent that will get it, and the skills it names that were not found.
// `workflow`: the prompt asks the agent for a workflow.

// Starts the agents and commands of stages, and starts them again after a restart or a resume.
export class StageLauncher {
  constructor(private readonly run: StageRun) {}

  markStarted(task: Task, stage: Stage, payload: Record<string, unknown>): boolean {
    const started = this.run.tasks.applyEvent(task.id, {
      source: 'system',
      type: 'stage.started',
      payload: { stage: stage.id, ...payload },
    }).accepted
    // The turn end of an earlier report of this stage never came; it must not swallow one of
    // this round.
    const key = `${task.id}/${terminalOfStage(this.run.tasks.stepsOf(task), stage.id)}`
    if (started && this.run.reported.get(key) === stage.id) {
      this.run.reported.delete(key)
    }
    return started
  }

  skillContext(task: Task, agent: string): SkillContext {
    const repo = this.run.repoPath(task.repo)
    return { agent, workspace: task.workspace ?? repo, repo, home: this.run.home }
  }

  private withSkills(text: string, task: Task, agent: string): { text: string; missing: string[] } {
    return fillSkills(text, this.skillContext(task, agent))
  }

  // The prompt of a stage that an agent works on: an agent stage, or a testing stage with no
  // command.
  promptOf(task: Task, stage: Stage, cause?: TaskEvent): PromptFor {
    if (stage.kind !== 'agent') {
      return (agent) => this.withSkills(stagePrompt(stage, task), task, agent)
    }
    const reason = sendBackReason(cause)
    return (agent) => {
      const workflow = runsWorkflow(stage, task.workflowStages, agent)
      const filled = this.withSkills(stagePrompt(stage, task, { workflow }), task, agent)
      const text = reason ? `${filled.text}\nWhat to change: ${reason}` : filled.text
      return { ...filled, text, workflow }
    }
  }

  async startAgent(
    task: Task,
    step: Step,
    stage: Stage,
    promptFor: PromptFor,
    resumed = false,
  ): Promise<void> {
    if (task.workspace === null) {
      this.run.waitingForWorkspace.add(task.id)
      return
    }
    // The agent may not start before the tasks it depends on are done. The stage stays ready and
    // starts when they are.
    if (waitingOn(task, this.run.tasks.listTasks()).length > 0) {
      this.run.waitingForTasks.add(task.id)
      return
    }
    if (task.withoutAgent === true && !this.run.chosenAgents.has(task.id)) {
      return
    }
    await excludeStageDir(task.workspace).catch(() => undefined)
    const head = await currentHead(task.workspace)
    const own = stageAgent(step, stage)
    const agent = own.agent ?? this.run.chosenAgents.get(task.id) ?? task.agent ?? 'claude-code'
    const terminal = terminalOfStage(this.run.tasks.stepsOf(task), stage.id)
    // A prompt typed to the agent took the stage up again: the agent has its prompt. With no
    // agent left (the app restarted), Claude Code goes on with its last session of the stage.
    const running = resumed && this.run.shells.agentRuns(task.id, terminal)
    const session =
      resumed && !running ? this.lastSession(task, stage.id, agent, terminal) : undefined
    const prompt = running || session !== undefined ? undefined : promptFor(agent)
    // The stage card says which skills of the prompt the agent did not get, and whether it asked
    // for a workflow.
    const missing = prompt?.missing.length ? { missingSkills: prompt.missing, agent } : {}
    const workflow = prompt?.workflow === true ? { workflow: true } : {}
    if (!this.markStarted(task, stage, { head, ...missing, ...workflow }) || running) {
      return
    }
    this.launch(task.id, stage.id, agent, terminal, prompt?.text ?? '', session, own)
  }

  // A person resumed a stage that waited for them. Its agent may be gone, for example after a
  // restart, and then starts again. One that still runs and ended its turn waits at its prompt, so
  // it is asked to go on. One that asked gets nothing: the Enter would answer its menu.
  resumeStage(taskId: string, event: TaskEvent): void {
    const task = this.run.tasks.getTask(taskId)
    const step = this.run.stepOf(task)
    const stage = stagesOf(step).find((candidate) => candidate.id === event.payload.stage)
    if (stage === undefined) {
      return
    }
    const terminal = terminalOfStage(this.run.tasks.stepsOf(task), stage.id)
    if (!this.run.shells.agentRuns(task.id, terminal)) {
      this.relaunch(task, step, stage)
    } else if (this.endedTurn(task.id, stage.id)) {
      this.run.terminals.paste(task.id, GO_ON_PROMPT, terminal)
    }
  }

  // True when the last thing the stage's agent did was to end its turn. Only the agent's own
  // events count: a CI result, an edit, a restart's launch or this resume leave it there.
  private endedTurn(taskId: string, stageId: string): boolean {
    for (const { accepted, event } of [...this.run.tasks.listEvents(taskId)].reverse()) {
      const stage = event.payload.stage
      const otherStage = typeof stage === 'string' && stage !== stageId
      if (!accepted || otherStage || event.source !== 'agent' || KEEPS_TURN.includes(event.type)) {
        continue
      }
      // After an API error the agent waits at its prompt, as after a turn end.
      return event.type === 'agent.finished' || event.type === 'agent.failed'
    }
    return false
  }

  // Agents end with the daemon. At its start, each stage an agent worked on gets its agent back,
  // and the task keeps its state. A stage in review gets it too, so you can ask it for changes.
  resumeAgents(): void {
    const all = this.run.tasks.listTasks()
    for (const task of all) {
      if (task.state === 'done' || task.state === 'cancelled') {
        continue
      }
      try {
        const step = this.run.stepOf(task)
        for (const stage of stagesOf(step)) {
          this.resumeLane(task, step, stage, all)
        }
      } catch {
        // A stage the settings no longer have: the task waits for a person, the daemon starts.
      }
    }
    this.run.openShells?.releaseHeld()
  }

  private resumeLane(task: Task, step: Step, stage: Stage, all: readonly Task[]): void {
    const state = laneState(task, step, stage.id)
    // A stage that waited for its workspace or for other tasks waits again: the restart
    // emptied those lists. Checked in the order startAgent and startCommand check.
    if (state === 'ready' && (stage.kind === 'agent' || stage.kind === 'testing')) {
      this.waitAgain(task, stage, all)
      return
    }
    // The restart killed the shell of a test command, so its result never comes. It runs
    // again. A stage that waits for CI is resumeCiChecks' work.
    if (
      stage.kind === 'testing' &&
      stage.command !== undefined &&
      state === 'running' &&
      !stagesWaitingForCi(this.run.tasks.listEvents(task.id)).includes(stage.id)
    ) {
      this.runCommandAgain(task, step, stage)
      return
    }
    const working = RESUMED_STATES.includes(state)
    // No agent Natamux started for it, or a task left to a person: a person runs their own
    // in the shell.
    const agent = task.withoutAgent === true ? null : (stageAgent(step, stage).agent ?? task.agent)
    // An agent the person quit before the app closed stays quit; the stage can be resumed.
    const terminal = terminalOfStage(this.run.tasks.stepsOf(task), stage.id)
    const quit = this.run.openShells?.wasRunning(task.id, terminal) === false
    if (working && agent !== null && isLaunchable(agent) && !quit) {
      // In a group, the newest session of the folder may be the other stage's.
      this.relaunch(task, step, stage, !isGroup(step), true)
    }
  }

  private waitAgain(task: Task, stage: Stage, all: readonly Task[]): void {
    if (task.workspace === null) {
      this.run.waitingForWorkspace.add(task.id)
    } else if (runByAgent(stage) && waitingOn(task, all).length > 0) {
      this.run.waitingForTasks.add(task.id)
    }
  }

  private runCommandAgain(task: Task, step: Step, stage: Stage): void {
    const stopped = this.run.tasks.applyEvent(task.id, {
      source: 'system',
      type: 'agent.stopped',
      payload: { stage: stage.id },
    })
    if (stopped.accepted) {
      this.run.background(this.run.startStage(task.id, step, stage))
    }
  }

  // Starts the agent of a stage again when it is gone. It goes on with the last session it
  // reported in its shell; Claude Code also with its session of the stage, or with `newest` the
  // newest of the folder. With none, it gets the stage prompt again.
  // At the daemon's start (`restart`) its pane shows a spinner until the agent runs.
  private relaunch(task: Task, step: Step, stage: Stage, newest = false, restart = false): void {
    if (!runByAgent(stage) || task.workspace === null) {
      return
    }
    const terminal = terminalOfStage(this.run.tasks.stepsOf(task), stage.id)
    if (this.run.shells.agentRuns(task.id, terminal)) {
      return
    }
    const own = stageAgent(step, stage)
    const agent = own.agent ?? task.agent ?? 'claude-code'
    const session =
      this.run.openShells?.sessionOf(task.id, terminal, agent) ??
      this.lastSession(task, stage.id, agent, terminal) ??
      (newest && agent === 'claude-code' ? LAST_SESSION : undefined)
    const text = session === undefined ? this.promptOf(task, stage)(agent).text : ''
    this.launch(task.id, stage.id, agent, terminal, text, session, own)
    if (restart) {
      this.run.openShells?.markStarting(task.id, terminal, agent)
    }
  }

  // The last session that Claude Code reported from this stage. Other agents start new ones.
  private lastSession(
    task: Task,
    stageId: string,
    agent: string,
    terminal: string,
  ): string | undefined {
    if (agent !== 'claude-code') {
      return undefined
    }
    const steps = this.run.tasks.stepsOf(task)
    const shellOf = (id: string) => {
      return findStep(steps, id) === undefined ? undefined : terminalOfStage(steps, id)
    }
    for (const { event } of [...this.run.tasks.listEvents(task.id)].reverse()) {
      // A session from before another agent was started in this shell is not this agent's. The
      // own agent of a stage in another shell does not count.
      const launchStage = event.payload.stage
      const launchShell = event.payload.terminal
      const otherShell =
        (typeof launchStage === 'string' &&
          launchStage !== stageId &&
          shellOf(launchStage) !== terminal) ||
        (typeof launchShell === 'string' && launchShell !== terminal)
      if (event.type === 'agent.launched' && event.payload.agent !== agent && !otherShell) {
        return undefined
      }
      const session = event.payload.sessionId
      const fromStage = event.source === 'agent' && event.payload.stage === stageId
      if (fromStage && typeof session === 'string' && session !== '') {
        return session
      }
    }
    return undefined
  }

  // Gives the prompt to the agent that runs in the terminal, else starts the agent there.
  private launch(
    taskId: string,
    stageId: string,
    agent: string,
    terminal: string,
    text: string,
    session?: string,
    own: Pick<Stage, 'agent' | 'args'> = {},
  ): void {
    try {
      if (session === undefined && this.run.shells.agentRuns(taskId, terminal)) {
        this.run.terminals.paste(taskId, text, terminal)
      } else {
        const none = own.agent === undefined && own.args === undefined
        this.run.agents.start(
          taskId,
          agent,
          text,
          terminal,
          session,
          none ? {} : { args: own.args, stage: stageId },
        )
        this.run.launched.set(`${taskId}/${terminal}`, { agent, at: this.run.now() })
      }
    } catch (error) {
      this.failStage(taskId, stageId, error instanceof Error ? error.message : String(error))
    }
  }

  // An agent started by a person: Start agent in the dashboard, the CLI, or the tries of a fan-out.
  // A stage that waits for its agent starts it, with the stage prompt. An agent that runs already
  // is left alone, since its command would be typed into it; a prompt goes to it instead. A task
  // with no workspace gets one from `makeWorkspace` first.
  async startByHand(
    taskId: string,
    agent: string,
    prompt?: string,
    makeWorkspace?: () => Promise<unknown>,
  ): Promise<Task> {
    this.run.tasks.setWithoutAgent(taskId, false)
    if (makeWorkspace !== undefined && this.run.tasks.getTask(taskId).workspace === null) {
      // The new workspace starts the stage that waits for it. It must start the agent picked
      // here, not the task's own.
      await this.startInNewWorkspace(taskId, agent, makeWorkspace)
    }
    // A new workspace may be starting the stage right now.
    await this.run.startWork.get(taskId)
    if (prompt === undefined && this.continueFailed(taskId)) {
      return this.run.tasks.getTask(taskId)
    }
    if (prompt === undefined && !this.run.shells.agentRuns(taskId)) {
      await this.startLanesWith(taskId, agent)
    }
    return this.promptOrStart(taskId, agent, prompt)
  }

  private async startInNewWorkspace(
    taskId: string,
    agent: string,
    makeWorkspace: () => Promise<unknown>,
  ): Promise<void> {
    this.run.chosenAgents.set(taskId, agent)
    try {
      await makeWorkspace()
      await this.run.startWork.get(taskId)
    } finally {
      this.run.chosenAgents.delete(taskId)
    }
  }

  // The stages that wait for an agent start the one a person picked.
  private async startLanesWith(taskId: string, agent: string): Promise<void> {
    this.run.chosenAgents.set(taskId, agent)
    try {
      await this.run.startLanes(taskId, ['draft', 'ready'])
    } finally {
      this.run.chosenAgents.delete(taskId)
    }
  }

  // The prompt goes to the agent that runs. With none, the picked agent starts in Shell 1.
  private promptOrStart(taskId: string, agent: string, prompt: string | undefined): Task {
    if (this.run.shells.agentRuns(taskId)) {
      if (prompt !== undefined) {
        this.run.terminals.paste(taskId, prompt)
      }
      return this.run.tasks.getTask(taskId)
    }
    const task = this.run.agents.start(taskId, agent, prompt)
    this.run.launched.set(`${taskId}/${MAIN_TERMINAL}`, { agent, at: this.run.now() })
    return task
  }

  // The stages of the current step whose agent failed go on where they stopped, as Resume does:
  // resumeStage asks an agent that still runs to go on, or starts the stage's agent again on its
  // last session. False when no such stage failed.
  private continueFailed(taskId: string): boolean {
    const task = this.run.tasks.getTask(taskId)
    const step = this.run.stepOf(task)
    const failed = stagesOf(step).filter(
      (stage) => runByAgent(stage) && laneState(task, step, stage.id) === 'failed',
    )
    for (const stage of failed) {
      this.run.tasks.applyEvent(taskId, {
        source: 'human',
        type: 'agent.resumed',
        payload: { stage: stage.id },
      })
    }
    return failed.length > 0
  }

  // A command whose exit code decides the stage, such as the test command.
  async startCommand(task: Task, stage: Stage, command: string) {
    if (task.workspace === null) {
      this.run.waitingForWorkspace.add(task.id)
      return
    }
    if (!this.markStarted(task, stage, {})) {
      return
    }
    // The command runs in the same terminal each time, so testing again adds no shell.
    const previous = this.run.commandTerminals.get(task.id)
    if (previous !== undefined) {
      this.run.terminals.closeOne(task.id, previous)
    }
    const open = this.run.terminals.list(task.id).map(Number)
    const terminal = previous ?? String(Math.max(1, ...open) + 1)
    this.run.commandTerminals.set(task.id, terminal)
    try {
      this.run.terminals.run(task.id, this.commandLine(task.id, stage.id, command), terminal)
    } catch (error) {
      this.failStage(task.id, stage.id, error instanceof Error ? error.message : String(error))
    }
  }

  // The command and the report of its result, as one line for the shell of the terminal. On a
  // POSIX system a script runs them, so a command such as `a; b` stays one unit and a failing
  // `stage pass` does not run `stage fail` too. PowerShell 5.1 has no && or ||, so on Windows the
  // line asks $? instead.
  private commandLine(taskId: string, stageId: string, command: string): string {
    const pass = `natamux stage pass ${taskId} --stage ${stageId}`
    const fail = `natamux stage fail ${taskId} --stage ${stageId}`
    if (this.run.platform === 'win32') {
      return `${command}; if ($?) { ${pass} } else { ${fail} }`
    }
    mkdirSync(this.run.dataDir, { recursive: true })
    const script = join(this.run.dataDir, `stage-command-${taskId}.sh`)
    const text = ['if (', command, '); then', `  ${pass}`, 'else', `  ${fail}`, 'fi', '']
    writeFileSync(script, text.join('\n'), { mode: 0o600 })
    return `sh ${quoteArgument(script, this.run.platform)}`
  }

  failStage(taskId: string, stage: string, reason: string): void {
    this.run.tasks.applyEvent(taskId, {
      source: 'system',
      type: 'stage.check-failed',
      payload: { stage, reasons: [reason], final: true },
    })
  }
}
