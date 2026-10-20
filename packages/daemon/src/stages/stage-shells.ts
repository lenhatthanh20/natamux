import { basename } from 'node:path'
import {
  canEndShell,
  ENDED_SHELL_LINES,
  type EndedShell,
  endsShell1,
  type Stage,
  type Step,
  stagesOf,
  stepToRun,
  type Task,
  type TaskEvent,
  terminalOfStage,
} from '@natamux/core'
import { agentOfCommand, LAST_SESSION, type LaunchableAgent } from '../agents/agent-launch.js'
import { AGENT_EXITED_TITLE } from '../agents/agent-runner.js'
import { readEndedShells, saveEndedShells } from '../terminals/ended-shells.js'
import { MAIN_TERMINAL, SHELLS } from '../terminals/terminal-limits.js'
import { laneState, RESUMED_STATES, StageActionError } from './stage-actions.js'
import type { StageRun } from './stage-run.js'

// After the launch line is typed, the shell stays in the foreground until the agent is up.
export const LAUNCH_GRACE_MS = 30_000

// How long a finishing task waits for a shell's output to reach its screen before it keeps the
// screen as it is. A flood must not keep the agent running.
const SCREEN_WAIT_MS = 1000

function settledOrLate(settled: Promise<void>): Promise<void> {
  let timer: NodeJS.Timeout | undefined
  const late = new Promise<void>((resolve) => {
    timer = setTimeout(resolve, SCREEN_WAIT_MS)
  })
  return Promise.race([settled, late]).finally(() => clearTimeout(timer))
}

// The escape sequences that set the window title, as the Windows console passes them on.
const AGENT_EXITED_MARKS = [`\x1b]0;${AGENT_EXITED_TITLE}`, `\x1b]2;${AGENT_EXITED_TITLE}`]
// Enough of the last output to find a mark that comes in two pieces.
const MARK_TAIL_CHARS = 64

// The shells of the stages: which agent runs where, closing them when a stage or a task ends, and
// bringing them back after a restart.
export class StageShells {
  constructor(private readonly run: StageRun) {}

  // A stage set to end its shell closes it once its lane is done. This runs before the next stage
  // starts, so a next stage in the same shell starts a new agent. A finished task is left to
  // `finish`, which keeps the screens.
  endStageShell(task: Task, event: TaskEvent): void {
    const stageId = event.payload.stage
    if (typeof stageId !== 'string' || task.state === 'done' || task.state === 'cancelled') {
      return
    }
    // A planned skip never started the stage, so its shell may hold the agent of the stage before.
    if (event.payload.planned === true) {
      return
    }
    const steps = this.run.tasks.stepsOf(task)
    const step = stepToRun(steps, stageId)
    const stage = step && stagesOf(step).find((each) => each.id === stageId)
    if (step === undefined || stage === undefined) {
      return
    }
    // Still in the step and not done: a stage that waits for approval, or a lane still at work.
    if (task.stage === step.id && task.lanes[stageId] !== 'done') {
      return
    }
    this.closeStageShell(task.id, steps, stage)
  }

  // A send back is the end of the stages it leaves, as a pass is. One that goes back inside the
  // same step leaves none: the stages there run on or run again.
  endLeftShells(task: Task): void {
    const left = this.run.leaving.get(task.id)
    if (left === undefined || task.stage === left.id) {
      return
    }
    const steps = this.run.tasks.stepsOf(task)
    for (const stage of stagesOf(left)) {
      this.closeStageShell(task.id, steps, stage)
    }
  }

  // Ends the agent or command of a stage set to end its shell, and closes the shell. Shell 1 keeps
  // the task's agent and is never closed.
  private closeStageShell(taskId: string, steps: readonly Step[], stage: Stage): void {
    if (stage.endShell !== true || !canEndShell(stage) || endsShell1(steps, stage)) {
      return
    }
    const command = stage.kind === 'testing' && stage.command !== undefined
    const terminal = command
      ? this.run.commandTerminals.get(taskId)
      : terminalOfStage(steps, stage.id)
    if (terminal === undefined) {
      return
    }
    if (command) {
      this.run.commandTerminals.delete(taskId)
    }
    this.run.terminals.closeOne(taskId, terminal)
    // The killed agent never ends its turn, so its report would swallow the first turn end of the
    // next agent in this shell.
    const key = `${taskId}/${terminal}`
    this.run.reported.delete(key)
    this.run.launched.delete(key)
    this.run.outputTails.delete(key)
  }

  // A task that is done or cancelled needs its agent and its shells no more. A deleted task keeps
  // nothing; a finished one keeps the last screens of its shells for the wrap-up.
  finish(taskId: string, options: { keepScreens: boolean }): void {
    const agents = new Map<string, { agent: string; session: string | null }>()
    for (const terminal of this.run.terminals.list(taskId)) {
      const launched = this.run.launched.get(`${taskId}/${terminal}`)
      if (launched !== undefined) {
        // Read now: the list of open shells forgets the task below.
        const session = this.run.openShells?.sessionOf(taskId, terminal, launched.agent) ?? null
        agents.set(terminal, { agent: launched.agent, session })
      }
    }
    if (options.keepScreens) {
      this.run.background(this.endShells(taskId, agents))
    } else {
      this.run.terminals.close(taskId)
    }
    for (const key of [...this.run.launched.keys(), ...this.run.reported.keys()]) {
      if (key.startsWith(`${taskId}/`)) {
        this.run.launched.delete(key)
        this.run.reported.delete(key)
        this.run.outputTails.delete(key)
      }
    }
    this.run.commandTerminals.delete(taskId)
    this.run.waitingForWorkspace.delete(taskId)
    this.run.waitingForTasks.delete(taskId)
    this.run.openShells?.forget(taskId)
  }

  // Keeps the last screen of each shell, then ends it. A shell where a program that is no agent
  // runs, such as a dev server, stays until it is idle. Without a list of processes nothing tells
  // an agent from a dev server, so such a shell ends, as all of them did before.
  private async endShells(
    taskId: string,
    launched: ReadonlyMap<string, { agent: string; session: string | null }>,
  ): Promise<void> {
    const endedAt = new Date(this.run.now()).toISOString()
    const finishing = this.run.tasks.listTasks().find((each) => each.id === taskId)
    const stages = finishing === undefined ? new Map<string, string>() : this.lastStages(finishing)
    const shells: EndedShell[] = []
    const kept = new Set<string>()
    for (const terminal of this.run.terminals.list(taskId)) {
      await settledOrLate(this.run.terminals.settled(taskId, terminal))
      const commands = await this.run.terminals.commands(taskId, terminal).catch(() => undefined)
      const text = this.run.terminals.lastScreen(taskId, terminal, ENDED_SHELL_LINES)
      if (text === undefined) {
        // Closed meanwhile.
        continue
      }
      const agent = launched.get(terminal)?.agent ?? this.agentOfCommands(commands)
      const session = launched.get(terminal)?.session ?? null
      const stage = stages.get(terminal) ?? null
      shells.push({ name: terminal, agent, stage, session, endedAt, text })
      const program = this.run.terminals.foreground(taskId, terminal)
      const busy = program !== undefined && !SHELLS.includes(basename(program))
      if (busy && agent === null && commands !== undefined) {
        kept.add(terminal)
      }
    }
    // A task deleted meanwhile keeps nothing, and its shells are closed already. One started again
    // meanwhile may use these shells.
    const task = this.run.tasks.listTasks().find((each) => each.id === taskId)
    if (task === undefined || (task.state !== 'done' && task.state !== 'cancelled')) {
      return
    }
    // Before the shells end: their end tells the dashboards to read the screens. The first finish
    // wins, so a later event does not replace the screens with newer, empty shells.
    if (shells.length > 0 && readEndedShells(this.run.dataDir, taskId).length === 0) {
      saveEndedShells(this.run.dataDir, taskId, shells)
    }
    for (const { name } of shells) {
      if (kept.has(name)) {
        this.run.terminals.endWhenIdle(taskId, name)
      } else {
        this.run.terminals.closeOne(taskId, name)
      }
    }
  }

  // The stage that started last in each shell, from the task's events.
  private lastStages(task: Task): Map<string, string> {
    const steps = this.run.tasks.stepsOf(task)
    const stages = new Map<string, string>()
    for (const { accepted, event } of this.run.tasks.listEvents(task.id)) {
      const stage = event.payload.stage
      if (accepted && event.type === 'stage.started' && typeof stage === 'string') {
        stages.set(terminalOfStage(steps, stage), stage)
      }
    }
    return stages
  }

  private agentOfCommands(commands: readonly string[] | undefined): string | null {
    const programOf = (agent: LaunchableAgent) => this.run.agents.programOf(agent)
    for (const line of commands ?? []) {
      const agent = agentOfCommand(line, programOf)
      if (agent !== undefined) {
        return agent
      }
    }
    return null
  }

  // At the daemon's start, before the dashboards connect: the shells that were open come back
  // once a dashboard opens their task. Those of the stages whose agent resumeAgents starts at once
  // are held until it ran. A finished task, or one without a workspace, keeps none.
  loadOpenShells(): void {
    for (const task of this.run.tasks.listTasks()) {
      if (task.state === 'done' || task.state === 'cancelled' || task.workspace === null) {
        this.run.openShells?.forget(task.id)
        continue
      }
      let working: string[] = []
      try {
        const steps = this.run.tasks.stepsOf(task)
        const step = this.run.stepOf(task)
        working = stagesOf(step)
          .filter(({ id }) => RESUMED_STATES.includes(laneState(task, step, id)))
          .map(({ id }) => terminalOfStage(steps, id))
      } catch {
        // A stage the settings no longer have: every shell waits for a dashboard.
      }
      this.run.openShells?.load(task.id, working)
    }
  }

  // A dashboard attaches to a shell of a task, so the task is open: every agent of it that waits to
  // come back after a restart goes on with its session, with no prompt, the attached shell's
  // first. A plain shell is left to its own attach.
  restoreTask(taskId: string, attached: string): void {
    const waiting = this.run.openShells?.waiting(taskId) ?? []
    for (const terminal of [attached, ...waiting.filter((name) => name !== attached)]) {
      this.restoreShell(taskId, terminal, terminal === attached)
    }
  }

  // A person reopens the agent of Terminal 1 of a done or cancelled task, to ask it something. It
  // goes on with its last session and gets no prompt. The task keeps its state and its agent.
  reopenAgent(taskId: string): Task {
    const task = this.run.tasks.getTask(taskId)
    if (task.state !== 'done' && task.state !== 'cancelled') {
      throw new StageActionError(`${taskId} is not done or cancelled`)
    }
    if (task.workspace === null) {
      throw new StageActionError(`${taskId} has no workspace`)
    }
    if (this.agentRuns(taskId, MAIN_TERMINAL)) {
      throw new StageActionError(`An agent runs in Terminal 1 of ${taskId} already`)
    }
    const ended = readEndedShells(this.run.dataDir, taskId).find(
      (shell) => shell.name === MAIN_TERMINAL && shell.agent !== null,
    )
    const agent = ended?.agent ?? task.agent
    if (agent === null) {
      throw new StageActionError(`No agent ran in Terminal 1 of ${taskId}`)
    }
    // The session it reported, else Claude Code's last one of the stage that ran there last, else
    // the newest of the folder.
    const stage = ended?.stage ?? this.lastStages(task).get(MAIN_TERMINAL)
    const ofStage =
      stage === undefined
        ? undefined
        : this.run.launcher.lastSession(task, stage, agent, MAIN_TERMINAL)
    const session = ended?.session ?? ofStage ?? LAST_SESSION
    const started = this.run.agents.start(taskId, agent, undefined, MAIN_TERMINAL, session, {
      restored: true,
    })
    this.run.launched.set(`${taskId}/${MAIN_TERMINAL}`, { agent, at: this.run.now() })
    this.run.openShells?.markStarting(taskId, MAIN_TERMINAL, agent)
    return started
  }

  // An agent that cannot start: the attached shell starts plain and keeps its agent in the list for
  // the next start; any other shell waits again.
  private restoreShell(taskId: string, terminal: string, attached: boolean): void {
    const open = this.run.openShells
    if (open === undefined || typeof open.restoring(taskId)[terminal] !== 'string') {
      return
    }
    const shell = open.take(taskId, terminal)
    if (shell === undefined || shell.agent === null) {
      return
    }
    try {
      const session = shell.session ?? LAST_SESSION
      this.run.agents.start(taskId, shell.agent, undefined, terminal, session, { restored: true })
      this.run.launched.set(`${taskId}/${terminal}`, { agent: shell.agent, at: this.run.now() })
      open.markStarting(taskId, terminal, shell.agent)
    } catch (error) {
      const reason = error instanceof Error ? error.message : String(error)
      console.warn(`Terminal ${terminal} of ${taskId} comes back without its agent: ${reason}`)
      if (!attached) {
        open.putBack(taskId, terminal)
      }
    }
  }

  // A prompt goes into a running agent only. Pasted into a shell, its lines would run as
  // commands. An agent runs when the program in the foreground is not a shell (an agent started
  // here, by hand, or before a restart), or for a short time after this daemon started one.
  agentRuns(taskId: string, terminal = MAIN_TERMINAL): boolean {
    const key = `${taskId}/${terminal}`
    if (!this.run.terminals.isOpen(taskId, terminal)) {
      // A new shell with this name has no agent in it.
      this.run.launched.delete(key)
      this.run.outputTails.delete(key)
      return false
    }
    const launchedAt = this.run.launched.get(key)?.at
    const program = this.run.terminals.foreground(taskId, terminal)
    if (program !== undefined && !SHELLS.includes(basename(program))) {
      return true
    }
    if (launchedAt === undefined) {
      return false
    }
    // Some platforms name only the shell (Windows does), so an agent started here counts.
    return (
      program === undefined ||
      this.run.platform === 'win32' ||
      this.run.now() - launchedAt < LAUNCH_GRACE_MS
    )
  }

  // The agent that runs in a terminal, for its pane header: the one its processes show. Where the
  // platform does not list them, or just after the launch, the one this daemon started there. Null
  // for a program that is no agent Natamux knows.
  async agentIn(taskId: string, terminal: string): Promise<string | null> {
    // A process that ends while `ps` runs must not fail the request.
    const commands = await this.run.terminals.commands(taskId, terminal).catch(() => undefined)
    const agent = this.agentOfCommands(commands)
    if (agent !== null) {
      return agent
    }
    const launched = this.run.launched.get(`${taskId}/${terminal}`)
    if (
      launched !== undefined &&
      (commands === undefined || this.run.now() - launched.at < LAUNCH_GRACE_MS)
    ) {
      return launched.agent
    }
    return null
  }

  // Output of a terminal of a task. On Windows an agent started here counts as running until its
  // launch line sets the window title that says it has ended (/exit, a crash).
  onOutput(taskId: string, terminal: string, data: Buffer): void {
    const key = `${taskId}/${terminal}`
    if (this.run.platform !== 'win32' || !this.run.launched.has(key)) {
      return
    }
    const text = (this.run.outputTails.get(key) ?? '') + data.toString()
    if (AGENT_EXITED_MARKS.some((mark) => text.includes(mark))) {
      this.run.launched.delete(key)
      this.run.outputTails.delete(key)
      return
    }
    this.run.outputTails.set(key, text.slice(-MARK_TAIL_CHARS))
  }
}
