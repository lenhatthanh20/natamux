import { mkdirSync, writeFileSync } from 'node:fs'
import { homedir } from 'node:os'
import { basename, join } from 'node:path'
import {
  type ContextSource,
  canEndShell,
  codeStageBefore,
  completeEvent,
  ENDED_SHELL_LINES,
  type EndedShell,
  type EventType,
  endsShell1,
  entersBranch,
  findStep,
  isBranch,
  isGroup,
  isNotTaken,
  isStage,
  offersCi,
  pathOf,
  plainText,
  RECORD_ONLY_EVENT_TYPES,
  runsWorkflow,
  type Stage,
  type Step,
  stageAgent,
  stageName,
  stageOfTerminal,
  stagesOf,
  stepIndex,
  stepToRun,
  type Task,
  type TaskEvent,
  type TaskState,
  type TerminalLane,
  terminalLane,
  terminalOfStage,
  waitingOn,
} from '@natamux/core'
import { agentOfCommand, isLaunchable, LAST_SESSION, type LaunchableAgent } from './agent-launch.js'
import { AGENT_EXITED_TITLE, type AgentRunner, quoteArgument } from './agent-runner.js'
import { fillSkills, resolveSkill, type SkillContext } from './agent-skills.js'
import { listAttachments, restoreAttachments, withAttachmentSection } from './attachments.js'
import { type CiResult, checkOnCi, ciBranchOf } from './ci-check.js'
import { readEndedShells, saveEndedShells } from './ended-shells.js'
import type { DaemonMessage, EventBus } from './event-bus.js'
import { deleteRemoteRef, type RunGh, runGh } from './git.js'
import type { OpenShells } from './open-shells.js'
import { currentHead, type RunCommand, runShellCommand, runStageChecks } from './stage-checks.js'
import {
  archiveStageFiles,
  excludeStageDir,
  readStageFile,
  restoreStageFiles,
  workingDir,
  writeStageFile,
} from './stage-files.js'
import { pickPath } from './stage-paths.js'
import { contextPrompt, manualContext, stagePrompt } from './stage-prompts.js'
import type { ApplyEventResult, TaskService } from './task-service.js'
import { MAIN_TERMINAL, SHELLS, type TerminalService } from './terminal-service.js'

// After the launch line is typed, the shell stays in the foreground until the agent is up.
export const LAUNCH_GRACE_MS = 30_000

// What Resume types to an agent that still runs and ended its turn.
export const GO_ON_PROMPT = 'Go on with the stage you are working on.'

// Events of an agent that leave it where it was: still at the end of its turn.
const KEEPS_TURN: EventType[] = ['agent.active']

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

export const STAGE_ACTIONS = [
  'approve',
  'reopen',
  'skip',
  'pass',
  'fail',
  'report',
  'start',
] as const
export type StageAction = (typeof STAGE_ACTIONS)[number]

export type StageActionInput = {
  // The stage the action is about. Needed inside a group when more than one stage could be meant.
  stage?: string
  // The terminal the action came from (`natamux done` in an agent's terminal).
  terminal?: string
  // reopen: the stage to go back to.
  to?: string
  // reopen and fail: why, for the agent and the log.
  reason?: string
  // The request came with the agent token of a task shell.
  byAgent?: boolean
}

export class StageActionError extends Error {
  constructor(message: string) {
    super(message)
    this.name = 'StageActionError'
  }
}

// An action that names something the task does not have, such as a stage to go back to.
export class StageInputError extends Error {
  constructor(message: string) {
    super(message)
    this.name = 'StageInputError'
  }
}

// An event as a client or the runner sends it, before the task service logs it.
type EventInput = Pick<TaskEvent, 'source' | 'type' | 'payload'>

// The states of a stage whose agent starts again when the daemon starts.
const RESUMED_STATES: readonly TaskState[] = ['running', 'needs-input', 'review']

function laneState(task: Pick<Task, 'state' | 'lanes'>, step: Step, stageId: string): TaskState {
  return isGroup(step) ? (task.lanes[stageId] ?? 'done') : task.state
}

// Events that end a stage and so may move the task on to the next step. A send back does not
// count: a person who sends the task back to a stage marked to skip wants it to run.
const ENDS_STAGE: readonly TaskEvent['type'][] = [
  'stage.approved',
  'review.approved',
  'stage.passed',
  'stage.checked',
  'stage.skipped',
]

// Rounds of "fix it" an agent may ask for on its own before a person decides.
export const MAX_AGENT_SEND_BACKS = 2

// The stages whose CI check started and has no result yet.
function stagesWaitingForCi(events: readonly { event: TaskEvent }[]): string[] {
  const waits = new Set<string>()
  for (const { event } of events) {
    const stage = event.payload.stage
    if (typeof stage !== 'string') {
      continue
    }
    if (event.type === 'stage.ci-started') {
      waits.add(stage)
    } else if (['stage.started', 'stage.checked', 'stage.check-failed'].includes(event.type)) {
      waits.delete(stage)
    }
  }
  return [...waits]
}

// A CI check that could not run, such as gh missing, is a failure no agent can fix.
function ciError(error: unknown): CiResult {
  const reason = error instanceof Error ? error.message : String(error)
  return { passed: false, reasons: [reason], passes: [], final: true }
}

// A stage an agent works on: an agent stage, or a testing stage with no command.
function runByAgent(stage: Stage): boolean {
  return stage.kind === 'agent' || (stage.kind === 'testing' && stage.command === undefined)
}

// Why a send back asked for changes, for the agent's next prompt.
function sendBackReason(cause: TaskEvent | undefined): string | undefined {
  if (
    cause === undefined ||
    (cause.type !== 'stage.reopened' && cause.type !== 'review.changes-requested')
  ) {
    return undefined
  }
  const reason = cause.payload.reason
  return typeof reason === 'string' && reason !== '' ? reason : undefined
}

export type IssueRef = {
  number: number
  // The repository of an issue address: "owner/name", for the closing line of a pull request.
  slug?: string
  // The same for `gh --repo`, with the host when it is not github.com.
  repo?: string
}

const ISSUE_ADDRESS = /^https?:\/\/([^/\s]+)\/([^/\s]+)\/([^/\s]+)\/issues\/(\d+)\/?(?:[?#]\S*)?$/
const ISSUE_KEY = /^(?:([\w.-]+\/[\w.-]+)#|#?)(\d+)$/

// An issue in "#42", "42", "owner/name#42" or an issue address (a comment link too).
export function parseIssueRef(ref: string): IssueRef | undefined {
  const text = ref.trim()
  const address = ISSUE_ADDRESS.exec(text)
  if (address !== null) {
    const [, host = '', owner = '', name = '', number = ''] = address
    const slug = `${owner}/${name}`
    return { number: Number(number), slug, repo: host === 'github.com' ? slug : `${host}/${slug}` }
  }
  const short = ISSUE_KEY.exec(text)
  if (short === null) {
    return undefined
  }
  const [, slug, number = ''] = short
  return slug === undefined
    ? { number: Number(number) }
    : { number: Number(number), slug, repo: slug }
}

// The issue number in "#42", "42" or an issue address.
export function issueNumber(ref: string): number | undefined {
  return parseIssueRef(ref)?.number
}

type GhIssue = {
  number?: number
  title?: string
  body?: string
  url?: string
  labels?: { name?: string }[]
  comments?: { author?: { login?: string }; body?: string; createdAt?: string }[]
}

export function issueContext(issue: GhIssue): string {
  const labels = (issue.labels ?? []).flatMap((label) => (label.name ? [label.name] : []))
  const comments = (issue.comments ?? []).map((comment) =>
    [
      `### ${comment.author?.login ?? 'someone'}${comment.createdAt ? `, ${comment.createdAt}` : ''}`,
      '',
      (comment.body ?? '').trim(),
    ].join('\n'),
  )
  return [
    `# ${issue.title ?? `Issue #${issue.number ?? ''}`}`,
    '',
    `Source: ${issue.url ?? ''}`,
    ...(labels.length === 0 ? [] : [`Labels: ${labels.join(', ')}`]),
    '',
    (issue.body ?? '').trim() || '(No description.)',
    ...(comments.length === 0 ? [] : ['', '## Comments', '', comments.join('\n\n')]),
    '',
  ].join('\n')
}

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

// A stage prompt for the agent that will get it, and the skills it names that were not found.
// `workflow`: the prompt asks the agent for a workflow.
type PromptFor = (agent: string) => { text: string; missing: string[]; workflow?: boolean }

// Moves tasks through their stages: starts each stage when the task reaches it, takes the report
// that a stage is done, checks the result and lets the gate decide (docs/design-task-stages.md).
export class StageRunner {
  private readonly tasks: TaskService
  private readonly terminals: TerminalService
  private readonly agents: AgentRunner
  private readonly bus: EventBus
  private readonly dataDir: string
  private readonly repoPath: (repo: string | null) => string
  private readonly gh: RunGh
  private readonly runCommand: RunCommand
  private readonly now: () => number
  private readonly platform: NodeJS.Platform
  private readonly home: string
  private readonly checkCi: (input: { worktree: string; branch: string }) => Promise<CiResult>
  private readonly openShells: OpenShells | undefined
  // Terminals whose agent reported a stage done, with that stage: the turn end that follows the
  // report is not a "stopped early". Kept by terminal, since the task may be at the next stage
  // when the turn ends.
  private readonly reported = new Map<string, string>()
  // Tasks whose agent stage waits for a workspace.
  private readonly waitingForWorkspace = new Set<string>()
  // Tasks whose agent stage waits for the tasks it depends on to be done.
  private readonly waitingForTasks = new Set<string>()
  // Terminals where this daemon started an agent: which one, and when.
  private readonly launched = new Map<string, { agent: string; at: number }>()
  // The end of the output of those terminals, on Windows.
  private readonly outputTails = new Map<string, string>()
  // The agent a person picked, while the stage it starts is starting.
  private readonly chosenAgents = new Map<string, string>()
  // The terminal where each task runs its stage command.
  private readonly commandTerminals = new Map<string, string>()
  // The step each task leaves by the send back it is sending now. The event does not say it.
  private readonly leaving = new Map<string, Step>()
  // The stages each task is starting now, so an action can answer with the task after them.
  private readonly startWork = new Map<string, Promise<void>>()
  private readonly starting = new Set<string>()
  private readonly pending = new Set<Promise<unknown>>()
  // Finished tasks whose CI branch was deleted, so later updates do not push again.
  private readonly ciCleaned = new Set<string>()
  // The CI checks that run now, by task and stage. A second report waits for the first check.
  private readonly ciRounds = new Set<string>()
  // The local checks that run now, by task, stage and round. Two reports at once (the agent's
  // and a person's) would run them twice in one worktree, and the second failure would count as
  // the agent's last try.
  private readonly checking = new Set<string>()
  private unsubscribe: (() => void) | undefined

  constructor(options: StageRunnerOptions) {
    this.tasks = options.tasks
    this.terminals = options.terminals
    this.agents = options.agents
    this.bus = options.bus
    this.dataDir = options.dataDir
    this.repoPath = options.repoPath
    this.gh = options.gh ?? runGh
    this.runCommand = options.runCommand ?? runShellCommand
    this.checkCi = options.checkCi ?? ((input) => checkOnCi(input))
    this.now = options.now ?? Date.now
    this.platform = options.platform ?? process.platform
    this.home = options.home ?? homedir()
    this.openShells = options.openShells
  }

  start(): void {
    this.unsubscribe = this.bus.subscribe((message) => this.onMessage(message))
    this.resumeCiChecks()
  }

  // A CI check that ran when the daemon stopped goes on: it pushes the work again and waits.
  private resumeCiChecks(): void {
    for (const task of this.tasks.listTasks()) {
      if (task.state === 'done' || task.state === 'cancelled') {
        continue
      }
      for (const stageId of stagesWaitingForCi(this.tasks.listEvents(task.id))) {
        this.startCiRound(task.id, stageId, this.lastStart(task.id, stageId).seq, [])
      }
    }
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
    this.startWork.set(taskId, work)
    this.background(
      work.finally(() => {
        if (this.startWork.get(taskId) === work) {
          this.startWork.delete(taskId)
        }
      }),
    )
  }

  // The step the task runs now. In a branch, that is the stage of the path it is at.
  private stepOf(task: Task): Step {
    const step = stepToRun(this.tasks.stepsOf(task), task.stage)
    if (step === undefined || isBranch(step)) {
      throw new StageActionError(`${task.id} is at an unknown stage "${task.stage}"`)
    }
    return step
  }

  // The stage a hook from this terminal is about, and its state. Null for a shell that no stage
  // uses, where the user started an agent by hand.
  laneFor(task: Task, terminal: string | undefined): { stage: string; state: TaskState } | null {
    const step = this.stepOf(task)
    const stage = stageOfTerminal(this.tasks.stepsOf(task), step, terminal ?? MAIN_TERMINAL)
    return stage === undefined ? null : { stage: stage.id, state: laneState(task, step, stage.id) }
  }

  // The stage an agent in this terminal works on, and its own state, for the dashboard.
  laneOfTerminal(task: Task, terminal: string): TerminalLane {
    return terminalLane(task, this.tasks.stepsOf(task), terminal)
  }

  // True once for the turn end that follows a report from this terminal.
  takeReport(taskId: string, terminal: string): boolean {
    return this.reported.delete(`${taskId}/${terminal}`)
  }

  private onMessage(message: DaemonMessage): void {
    if (message.kind === 'task.deleted') {
      this.finish(message.taskId, { keepScreens: false })
      return
    }
    if (message.kind === 'task.created') {
      const step = this.stepOf(message.task)
      if (stagesOf(step).some((stage) => stage.kind === 'context')) {
        this.startInBackground(message.task.id, ['draft', 'ready'])
      }
      return
    }
    if (message.kind !== 'task.updated' || !message.accepted) {
      return
    }
    const { task, event } = message
    // Each piece of side work below has its own try, so a failed one does not stop the next stage.
    if (['stage.approved', 'stage.passed', 'stage.checked'].includes(event.type)) {
      try {
        archiveStageFiles(this.dataDir, task)
      } catch (error) {
        console.error(`Could not archive the stage files of ${task.id}: ${String(error)}`)
      }
    }
    const finished = task.state === 'done' || task.state === 'cancelled'
    const usedCi = (task.ciStages ?? []).length > 0 && !this.ciCleaned.has(task.id)
    if (finished && usedCi && task.branch !== null) {
      this.ciCleaned.add(task.id)
      // The CI branch only held snapshots for checks. Gone already, or offline: only logged.
      const ref = ciBranchOf(task.branch)
      this.background(
        deleteRemoteRef(this.repoPath(task.repo), ref).catch((error: unknown) => {
          console.error(`Could not delete the CI branch ${ref} of ${task.id}: ${String(error)}`)
        }),
      )
    }
    if (event.type === 'workspace.created' && task.workspace !== null) {
      try {
        restoreStageFiles(this.dataDir, task)
      } catch (error) {
        console.error(`Could not restore the stage files of ${task.id}: ${String(error)}`)
      }
      try {
        restoreAttachments(this.dataDir, task)
      } catch (error) {
        console.error(`Could not restore the attachments of ${task.id}: ${String(error)}`)
      }
      this.background(
        excludeStageDir(task.workspace).catch((error: unknown) => {
          console.error(`Could not ignore the stage folder of ${task.id}: ${String(error)}`)
        }),
      )
      if (this.waitingForWorkspace.delete(task.id)) {
        this.startInBackground(task.id, ['ready', 'draft'])
      }
    }
    if (ENDS_STAGE.includes(event.type)) {
      this.endStageShell(task, event)
    }
    if (event.type === 'stage.reopened') {
      this.endLeftShells(task)
    }
    if (message.moved === true) {
      this.startInBackground(task.id, ['ready'], event)
    } else if (event.type === 'agent.resumed' && event.source !== 'agent') {
      // After this message has reached every listener, so its events come in order.
      this.background(Promise.resolve().then(() => this.resumeStage(task.id, event)))
    }
    const ended = task.state === 'done' || task.state === 'cancelled'
    if (ended && !RECORD_ONLY_EVENT_TYPES.includes(event.type)) {
      this.finish(task.id, { keepScreens: true })
    }
    if (task.state === 'done' || event.type === 'task.edited') {
      this.startWaiting()
    }
  }

  // A stage set to end its shell closes it once its lane is done. This runs before the next stage
  // starts, so a next stage in the same shell starts a new agent. A finished task is left to
  // `finish`, which keeps the screens.
  private endStageShell(task: Task, event: TaskEvent): void {
    const stageId = event.payload.stage
    if (typeof stageId !== 'string' || task.state === 'done' || task.state === 'cancelled') {
      return
    }
    // A planned skip never started the stage, so its shell may hold the agent of the stage before.
    if (event.payload.planned === true) {
      return
    }
    const steps = this.tasks.stepsOf(task)
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
  private endLeftShells(task: Task): void {
    const left = this.leaving.get(task.id)
    if (left === undefined || task.stage === left.id) {
      return
    }
    const steps = this.tasks.stepsOf(task)
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
    const terminal = command ? this.commandTerminals.get(taskId) : terminalOfStage(steps, stage.id)
    if (terminal === undefined) {
      return
    }
    if (command) {
      this.commandTerminals.delete(taskId)
    }
    this.terminals.closeOne(taskId, terminal)
    // The killed agent never ends its turn, so its report would swallow the first turn end of the
    // next agent in this shell.
    const key = `${taskId}/${terminal}`
    this.reported.delete(key)
    this.launched.delete(key)
    this.outputTails.delete(key)
  }

  // A task that is done or cancelled needs its agent and its shells no more. A deleted task keeps
  // nothing; a finished one keeps the last screens of its shells for the wrap-up.
  private finish(taskId: string, options: { keepScreens: boolean }): void {
    const agents = new Map<string, string>()
    for (const terminal of this.terminals.list(taskId)) {
      const launched = this.launched.get(`${taskId}/${terminal}`)
      if (launched !== undefined) {
        agents.set(terminal, launched.agent)
      }
    }
    if (options.keepScreens) {
      this.background(this.endShells(taskId, agents))
    } else {
      this.terminals.close(taskId)
    }
    for (const key of [...this.launched.keys(), ...this.reported.keys()]) {
      if (key.startsWith(`${taskId}/`)) {
        this.launched.delete(key)
        this.reported.delete(key)
        this.outputTails.delete(key)
      }
    }
    this.commandTerminals.delete(taskId)
    this.waitingForWorkspace.delete(taskId)
    this.waitingForTasks.delete(taskId)
    this.openShells?.forget(taskId)
  }

  // Keeps the last screen of each shell, then ends it. A shell where a program that is no agent
  // runs, such as a dev server, stays until it is idle. Without a list of processes nothing tells
  // an agent from a dev server, so such a shell ends, as all of them did before.
  private async endShells(taskId: string, launched: ReadonlyMap<string, string>): Promise<void> {
    const endedAt = new Date(this.now()).toISOString()
    const finishing = this.tasks.listTasks().find((each) => each.id === taskId)
    const stages = finishing === undefined ? new Map<string, string>() : this.lastStages(finishing)
    const shells: EndedShell[] = []
    const kept = new Set<string>()
    for (const terminal of this.terminals.list(taskId)) {
      await settledOrLate(this.terminals.settled(taskId, terminal))
      const commands = await this.terminals.commands(taskId, terminal).catch(() => undefined)
      const text = this.terminals.lastScreen(taskId, terminal, ENDED_SHELL_LINES)
      if (text === undefined) {
        // Closed meanwhile.
        continue
      }
      const agent = launched.get(terminal) ?? this.agentOfCommands(commands)
      shells.push({ name: terminal, agent, stage: stages.get(terminal) ?? null, endedAt, text })
      const program = this.terminals.foreground(taskId, terminal)
      const busy = program !== undefined && !SHELLS.includes(basename(program))
      if (busy && agent === null && commands !== undefined) {
        kept.add(terminal)
      }
    }
    // A task deleted meanwhile keeps nothing, and its shells are closed already. One started again
    // meanwhile may use these shells.
    const task = this.tasks.listTasks().find((each) => each.id === taskId)
    if (task === undefined || (task.state !== 'done' && task.state !== 'cancelled')) {
      return
    }
    // Before the shells end: their end tells the dashboards to read the screens. The first finish
    // wins, so a later event does not replace the screens with newer, empty shells.
    if (shells.length > 0 && readEndedShells(this.dataDir, taskId).length === 0) {
      saveEndedShells(this.dataDir, taskId, shells)
    }
    for (const { name } of shells) {
      if (kept.has(name)) {
        this.terminals.endWhenIdle(taskId, name)
      } else {
        this.terminals.closeOne(taskId, name)
      }
    }
  }

  // The stage that started last in each shell, from the task's events.
  private lastStages(task: Task): Map<string, string> {
    const steps = this.tasks.stepsOf(task)
    const stages = new Map<string, string>()
    for (const { accepted, event } of this.tasks.listEvents(task.id)) {
      const stage = event.payload.stage
      if (accepted && event.type === 'stage.started' && typeof stage === 'string') {
        stages.set(terminalOfStage(steps, stage), stage)
      }
    }
    return stages
  }

  private agentOfCommands(commands: readonly string[] | undefined): string | null {
    const programOf = (agent: LaunchableAgent) => this.agents.programOf(agent)
    for (const line of commands ?? []) {
      const agent = agentOfCommand(line, programOf)
      if (agent !== undefined) {
        return agent
      }
    }
    return null
  }

  // A task made with no agent: its stages that need one stay ready, and no agent starts in its
  // shell until a person starts one.
  leaveToPerson(taskId: string): void {
    this.tasks.setWithoutAgent(taskId, true)
  }

  // Starts the stages that waited for other tasks, once those are done.
  private startWaiting(): void {
    if (this.waitingForTasks.size === 0) {
      return
    }
    const all = this.tasks.listTasks()
    for (const id of [...this.waitingForTasks]) {
      const task = all.find((candidate) => candidate.id === id)
      if (task === undefined || waitingOn(task, all).length === 0) {
        this.waitingForTasks.delete(id)
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
    const task = this.tasks.getTask(taskId)
    const step = this.stepOf(task)
    for (const stage of stagesOf(step)) {
      if (states.includes(laneState(task, step, stage.id))) {
        await this.startStage(task.id, step, stage, cause)
      }
    }
  }

  // The stages of the current step that the user marked to skip, and that are not done.
  private markedLanes(taskId: string): Stage[] {
    const task = this.tasks.getTask(taskId)
    const marked = task.skipStages ?? []
    const step = this.stepOf(task)
    return stagesOf(step).filter(
      (stage) => marked.includes(stage.id) && laneState(task, step, stage.id) !== 'done',
    )
  }

  // Skips the marked stages of the current step. True when that ended the step.
  private async skipMarked(taskId: string): Promise<boolean> {
    const before = this.tasks.getTask(taskId)
    for (const stage of this.markedLanes(taskId)) {
      await this.applyEnding(taskId, {
        source: 'system',
        type: 'stage.skipped',
        payload: { stage: stage.id, planned: true },
      })
    }
    const after = this.tasks.getTask(taskId)
    return after.stage !== before.stage || after.state === 'done'
  }

  private async startStage(taskId: string, step: Step, stage: Stage, cause?: TaskEvent) {
    const key = `${taskId}/${stage.id}`
    if (this.starting.has(key)) {
      return
    }
    this.starting.add(key)
    try {
      const task = this.tasks.getTask(taskId)
      switch (stage.kind) {
        case 'context':
          await this.startContext(task, step, stage)
          return
        case 'agent': {
          const resumed = cause?.type === 'agent.resumed'
          await this.startAgent(task, step, stage, this.promptOf(task, stage, cause), resumed)
          return
        }
        case 'testing':
          if (stage.command === undefined) {
            await this.startAgent(task, step, stage, this.promptOf(task, stage))
          } else {
            await this.startCommand(task, stage, stage.command)
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

  private markStarted(task: Task, stage: Stage, payload: Record<string, unknown>): boolean {
    const started = this.tasks.applyEvent(task.id, {
      source: 'system',
      type: 'stage.started',
      payload: { stage: stage.id, ...payload },
    }).accepted
    // The turn end of an earlier report of this stage never came; it must not swallow one of
    // this round.
    const key = `${task.id}/${terminalOfStage(this.tasks.stepsOf(task), stage.id)}`
    if (started && this.reported.get(key) === stage.id) {
      this.reported.delete(key)
    }
    return started
  }

  private skillContext(task: Task, agent: string): SkillContext {
    const repo = this.repoPath(task.repo)
    return { agent, workspace: task.workspace ?? repo, repo, home: this.home }
  }

  private withSkills(text: string, task: Task, agent: string): { text: string; missing: string[] } {
    return fillSkills(text, this.skillContext(task, agent))
  }

  // The prompt of a stage that an agent works on: an agent stage, or a testing stage with no
  // command.
  private promptOf(task: Task, stage: Stage, cause?: TaskEvent): PromptFor {
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

  private async startAgent(
    task: Task,
    step: Step,
    stage: Stage,
    promptFor: PromptFor,
    resumed = false,
  ): Promise<void> {
    if (task.workspace === null) {
      this.waitingForWorkspace.add(task.id)
      return
    }
    // The agent may not start before the tasks it depends on are done. The stage stays ready and
    // starts when they are.
    if (waitingOn(task, this.tasks.listTasks()).length > 0) {
      this.waitingForTasks.add(task.id)
      return
    }
    if (task.withoutAgent === true && !this.chosenAgents.has(task.id)) {
      return
    }
    await excludeStageDir(task.workspace).catch(() => undefined)
    const head = await currentHead(task.workspace)
    const own = stageAgent(step, stage)
    const agent = own.agent ?? this.chosenAgents.get(task.id) ?? task.agent ?? 'claude-code'
    const terminal = terminalOfStage(this.tasks.stepsOf(task), stage.id)
    // A prompt typed to the agent took the stage up again: the agent has its prompt. With no
    // agent left (the app restarted), Claude Code goes on with its last session of the stage.
    const running = resumed && this.agentRuns(task.id, terminal)
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
  private resumeStage(taskId: string, event: TaskEvent): void {
    const task = this.tasks.getTask(taskId)
    const step = this.stepOf(task)
    const stage = stagesOf(step).find((candidate) => candidate.id === event.payload.stage)
    if (stage === undefined) {
      return
    }
    const terminal = terminalOfStage(this.tasks.stepsOf(task), stage.id)
    if (!this.agentRuns(task.id, terminal)) {
      this.relaunch(task, step, stage)
    } else if (this.endedTurn(task.id, stage.id)) {
      this.terminals.paste(task.id, GO_ON_PROMPT, terminal)
    }
  }

  // True when the last thing the stage's agent did was to end its turn. Only the agent's own
  // events count: a CI result, an edit, a restart's launch or this resume leave it there.
  private endedTurn(taskId: string, stageId: string): boolean {
    for (const { accepted, event } of [...this.tasks.listEvents(taskId)].reverse()) {
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
    const all = this.tasks.listTasks()
    for (const task of all) {
      if (task.state === 'done' || task.state === 'cancelled') {
        continue
      }
      try {
        const step = this.stepOf(task)
        for (const stage of stagesOf(step)) {
          const state = laneState(task, step, stage.id)
          // A stage that waited for its workspace or for other tasks waits again: the restart
          // emptied those lists. Checked in the order startAgent and startCommand check.
          if (state === 'ready' && (stage.kind === 'agent' || stage.kind === 'testing')) {
            if (task.workspace === null) {
              this.waitingForWorkspace.add(task.id)
            } else if (runByAgent(stage) && waitingOn(task, all).length > 0) {
              this.waitingForTasks.add(task.id)
            }
            continue
          }
          // The restart killed the shell of a test command, so its result never comes. It runs
          // again. A stage that waits for CI is resumeCiChecks' work.
          if (
            stage.kind === 'testing' &&
            stage.command !== undefined &&
            state === 'running' &&
            !stagesWaitingForCi(this.tasks.listEvents(task.id)).includes(stage.id)
          ) {
            const stopped = this.tasks.applyEvent(task.id, {
              source: 'system',
              type: 'agent.stopped',
              payload: { stage: stage.id },
            })
            if (stopped.accepted) {
              this.background(this.startStage(task.id, step, stage))
            }
            continue
          }
          const working = RESUMED_STATES.includes(state)
          // No agent Natamux started for it, or a task left to a person: a person runs their own
          // in the shell.
          const agent =
            task.withoutAgent === true ? null : (stageAgent(step, stage).agent ?? task.agent)
          if (working && agent !== null && isLaunchable(agent)) {
            // In a group, the newest session of the folder may be the other stage's.
            this.relaunch(task, step, stage, !isGroup(step), true)
          }
        }
      } catch {
        // A stage the settings no longer have: the task waits for a person, the daemon starts.
      }
    }
  }

  // Starts the agent of a stage again when it is gone. Claude Code goes on with its session of the
  // stage, or with `newest` the newest of the folder; other agents get the stage prompt again.
  // At the daemon's start (`restart`) its pane shows a spinner until the agent runs.
  private relaunch(task: Task, step: Step, stage: Stage, newest = false, restart = false): void {
    if (!runByAgent(stage) || task.workspace === null) {
      return
    }
    const terminal = terminalOfStage(this.tasks.stepsOf(task), stage.id)
    if (this.agentRuns(task.id, terminal)) {
      return
    }
    const own = stageAgent(step, stage)
    const agent = own.agent ?? task.agent ?? 'claude-code'
    const session =
      this.lastSession(task, stage.id, agent, terminal) ??
      (newest && agent === 'claude-code' ? LAST_SESSION : undefined)
    const text = session === undefined ? this.promptOf(task, stage)(agent).text : ''
    this.launch(task.id, stage.id, agent, terminal, text, session, own)
    if (restart) {
      this.openShells?.markStarting(task.id, terminal, agent)
    }
  }

  // At the daemon's start, before the dashboards connect: the shells that were open come back
  // once a dashboard attaches to them, except those of the stages whose agent resumeAgents starts
  // at once. A finished task, or one without a workspace, keeps none.
  loadOpenShells(): void {
    for (const task of this.tasks.listTasks()) {
      if (task.state === 'done' || task.state === 'cancelled' || task.workspace === null) {
        this.openShells?.forget(task.id)
        continue
      }
      let working: string[] = []
      try {
        const steps = this.tasks.stepsOf(task)
        const step = this.stepOf(task)
        working = stagesOf(step)
          .filter(({ id }) => RESUMED_STATES.includes(laneState(task, step, id)))
          .map(({ id }) => terminalOfStage(steps, id))
      } catch {
        // A stage the settings no longer have: every shell waits for a dashboard.
      }
      this.openShells?.load(task.id, working)
    }
  }

  // A dashboard attaches to a shell that waits to come back after a restart. Its agent goes on
  // with its session, with no prompt. A plain shell, or an agent that cannot start, is left to the
  // attach, which starts a plain shell.
  restoreShell(taskId: string, terminal: string): void {
    const shell = this.openShells?.take(taskId, terminal)
    if (shell === undefined || shell.agent === null || shell.session === null) {
      return
    }
    try {
      this.agents.start(taskId, shell.agent, undefined, terminal, shell.session, { restored: true })
      this.launched.set(`${taskId}/${terminal}`, { agent: shell.agent, at: this.now() })
      this.openShells?.markStarting(taskId, terminal, shell.agent)
    } catch (error) {
      const reason = error instanceof Error ? error.message : String(error)
      console.warn(`Shell ${terminal} of ${taskId} comes back without its agent: ${reason}`)
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
    const steps = this.tasks.stepsOf(task)
    const shellOf = (id: string) => {
      return findStep(steps, id) === undefined ? undefined : terminalOfStage(steps, id)
    }
    for (const { event } of [...this.tasks.listEvents(task.id)].reverse()) {
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
      if (session === undefined && this.agentRuns(taskId, terminal)) {
        this.terminals.paste(taskId, text, terminal)
      } else {
        const none = own.agent === undefined && own.args === undefined
        this.agents.start(
          taskId,
          agent,
          text,
          terminal,
          session,
          none ? {} : { args: own.args, stage: stageId },
        )
        this.launched.set(`${taskId}/${terminal}`, { agent, at: this.now() })
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
    this.tasks.setWithoutAgent(taskId, false)
    if (makeWorkspace !== undefined && this.tasks.getTask(taskId).workspace === null) {
      // The new workspace starts the stage that waits for it. It must start the agent picked
      // here, not the task's own.
      this.chosenAgents.set(taskId, agent)
      try {
        await makeWorkspace()
        await this.startWork.get(taskId)
      } finally {
        this.chosenAgents.delete(taskId)
      }
    }
    // A new workspace may be starting the stage right now.
    await this.startWork.get(taskId)
    if (prompt === undefined && this.continueFailed(taskId)) {
      return this.tasks.getTask(taskId)
    }
    if (prompt === undefined && !this.agentRuns(taskId)) {
      this.chosenAgents.set(taskId, agent)
      try {
        await this.startLanes(taskId, ['draft', 'ready'])
      } finally {
        this.chosenAgents.delete(taskId)
      }
    }
    if (this.agentRuns(taskId)) {
      if (prompt !== undefined) {
        this.terminals.paste(taskId, prompt)
      }
      return this.tasks.getTask(taskId)
    }
    const task = this.agents.start(taskId, agent, prompt)
    this.launched.set(`${taskId}/${MAIN_TERMINAL}`, { agent, at: this.now() })
    return task
  }

  // The stages of the current step whose agent failed go on where they stopped, as Resume does:
  // resumeStage asks an agent that still runs to go on, or starts the stage's agent again on its
  // last session. False when no such stage failed.
  private continueFailed(taskId: string): boolean {
    const task = this.tasks.getTask(taskId)
    const step = this.stepOf(task)
    const failed = stagesOf(step).filter(
      (stage) => runByAgent(stage) && laneState(task, step, stage.id) === 'failed',
    )
    for (const stage of failed) {
      this.tasks.applyEvent(taskId, {
        source: 'human',
        type: 'agent.resumed',
        payload: { stage: stage.id },
      })
    }
    return failed.length > 0
  }

  // A prompt goes into a running agent only. Pasted into a shell, its lines would run as
  // commands. An agent runs when the program in the foreground is not a shell (an agent started
  // here, by hand, or before a restart), or for a short time after this daemon started one.
  agentRuns(taskId: string, terminal = MAIN_TERMINAL): boolean {
    const key = `${taskId}/${terminal}`
    if (!this.terminals.isOpen(taskId, terminal)) {
      // A new shell with this name has no agent in it.
      this.launched.delete(key)
      this.outputTails.delete(key)
      return false
    }
    const launchedAt = this.launched.get(key)?.at
    const program = this.terminals.foreground(taskId, terminal)
    if (program !== undefined && !SHELLS.includes(basename(program))) {
      return true
    }
    if (launchedAt === undefined) {
      return false
    }
    // Some platforms name only the shell (Windows does), so an agent started here counts.
    return (
      program === undefined ||
      this.platform === 'win32' ||
      this.now() - launchedAt < LAUNCH_GRACE_MS
    )
  }

  // The agent that runs in a terminal, for its pane header: the one its processes show. Where the
  // platform does not list them, or just after the launch, the one this daemon started there. Null
  // for a program that is no agent Natamux knows.
  async agentIn(taskId: string, terminal: string): Promise<string | null> {
    // A process that ends while `ps` runs must not fail the request.
    const commands = await this.terminals.commands(taskId, terminal).catch(() => undefined)
    const agent = this.agentOfCommands(commands)
    if (agent !== null) {
      return agent
    }
    const launched = this.launched.get(`${taskId}/${terminal}`)
    if (
      launched !== undefined &&
      (commands === undefined || this.now() - launched.at < LAUNCH_GRACE_MS)
    ) {
      return launched.agent
    }
    return null
  }

  // Output of a terminal of a task. On Windows an agent started here counts as running until its
  // launch line sets the window title that says it has ended (/exit, a crash).
  onOutput(taskId: string, terminal: string, data: Buffer): void {
    const key = `${taskId}/${terminal}`
    if (this.platform !== 'win32' || !this.launched.has(key)) {
      return
    }
    const text = (this.outputTails.get(key) ?? '') + data.toString()
    if (AGENT_EXITED_MARKS.some((mark) => text.includes(mark))) {
      this.launched.delete(key)
      this.outputTails.delete(key)
      return
    }
    this.outputTails.set(key, text.slice(-MARK_TAIL_CHARS))
  }

  // A command whose exit code decides the stage, such as the test command.
  private async startCommand(task: Task, stage: Stage, command: string) {
    if (task.workspace === null) {
      this.waitingForWorkspace.add(task.id)
      return
    }
    if (!this.markStarted(task, stage, {})) {
      return
    }
    // The command runs in the same terminal each time, so testing again adds no shell.
    const previous = this.commandTerminals.get(task.id)
    if (previous !== undefined) {
      this.terminals.closeOne(task.id, previous)
    }
    const open = this.terminals.list(task.id).map(Number)
    const terminal = previous ?? String(Math.max(1, ...open) + 1)
    this.commandTerminals.set(task.id, terminal)
    try {
      this.terminals.run(task.id, this.commandLine(task.id, stage.id, command), terminal)
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
    if (this.platform === 'win32') {
      return `${command}; if ($?) { ${pass} } else { ${fail} }`
    }
    mkdirSync(this.dataDir, { recursive: true })
    const script = join(this.dataDir, `stage-command-${taskId}.sh`)
    const text = ['if (', command, '); then', `  ${pass}`, 'else', `  ${fail}`, 'fi', '']
    writeFileSync(script, text.join('\n'), { mode: 0o600 })
    return `sh ${quoteArgument(script, this.platform)}`
  }

  private failStage(taskId: string, stage: string, reason: string): void {
    this.tasks.applyEvent(taskId, {
      source: 'system',
      type: 'stage.check-failed',
      payload: { stage, reasons: [reason], final: true },
    })
  }

  private sourceOf(task: Task): ContextSource {
    const type = task.context?.source ?? 'manual'
    const skill = task.context?.skill
    if (type === 'skill' && skill) {
      return { type: 'skill', name: skill, skill }
    }
    const name = task.context?.name
    const offered = this.tasks
      .settingsOf(task.repo)
      .context.sources.filter((source) => source.type === type)
    // A task from before names were sent, or one whose source was renamed, takes the first.
    const configured =
      offered.find((source) => 'name' in source && source.name === name) ?? offered[0]
    return configured ?? (type === 'skill' || type === 'mcp' ? { type: 'manual' } : { type })
  }

  private async startContext(task: Task, step: Step, stage: Stage): Promise<void> {
    const source = this.sourceOf(task)
    if (source.type === 'skill' || source.type === 'mcp') {
      const attachments = listAttachments(this.dataDir, task.id)
      await this.startAgent(task, step, stage, (agent) => {
        const skill =
          source.type === 'skill'
            ? resolveSkill(source.skill, this.skillContext(task, agent))
            : undefined
        return {
          text: contextPrompt(source, task, attachments, skill),
          missing: source.type === 'skill' && skill?.kind === 'missing' ? [source.skill] : [],
        }
      })
      return
    }
    if (!this.markStarted(task, stage, { source: source.type })) {
      return
    }
    const output = stage.output ?? 'context.md'
    try {
      const text = source.type === 'github-issue' ? await this.readIssue(task) : manualContext(task)
      const attachments = listAttachments(this.dataDir, task.id)
      // The task may have got its workspace while the issue was read: write where it is now.
      writeStageFile(
        this.dataDir,
        this.tasks.getTask(task.id),
        output,
        withAttachmentSection(text, attachments),
      )
    } catch (error) {
      this.failStage(task.id, stage.id, error instanceof Error ? error.message : String(error))
      return
    }
    this.tasks.recordEvent(task.id, 'context.fetched', { stage: stage.id, source: source.type })
    await this.reportAndCheck(task.id, stage.id, 'system')
    // A context of the task's own title and description holds nothing new to read. Waiting for
    // approval would put every new backlog task in Needs you.
    const now = this.tasks.getTask(task.id)
    if (source.type === 'manual' && now.stage === stage.id && now.state === 'review') {
      await this.applyEnding(task.id, {
        source: 'system',
        type: 'stage.passed',
        payload: { stage: stage.id },
      })
    }
  }

  private async readIssue(task: Task): Promise<string> {
    const ref =
      parseIssueRef(task.context?.ref ?? '') ??
      (task.issue?.provider === 'github' ? parseIssueRef(task.issue.key) : undefined)
    if (ref === undefined) {
      throw new StageActionError('Give the issue number, such as #42, to read the context from')
    }
    // An address of another repository reads that repository's issue, not the same number here.
    const output = await this.gh(this.repoPath(task.repo), [
      'issue',
      'view',
      String(ref.number),
      ...(ref.repo === undefined ? [] : ['--repo', ref.repo]),
      '--json',
      'number,title,body,url,labels,comments',
    ])
    const issue = JSON.parse(output) as GhIssue
    if (this.tasks.getTask(task.id).issue === null && issue.url) {
      const number = issue.number ?? ref.number
      this.tasks.linkIssue(
        task.id,
        {
          provider: 'github',
          key: ref.slug === undefined ? `#${number}` : `${ref.slug}#${number}`,
          url: issue.url,
        },
        'system',
      )
    }
    return issueContext(issue)
  }

  // The stage an action is about: the one named, the one of the terminal, or the only stage of
  // the step that can take the action. A terminal of a stage that does not run now is refused: a
  // reviewer that sent the task back must not report Implement done.
  private resolveStage(
    task: Task,
    step: Step,
    input: StageActionInput,
    states: readonly TaskState[],
  ): Stage {
    const stages = stagesOf(step)
    if (input.stage !== undefined) {
      const named = stages.find((stage) => stage.id === input.stage)
      if (named === undefined) {
        throw new StageActionError(`${task.id} is not at the ${input.stage} stage now`)
      }
      return named
    }
    if (input.terminal !== undefined) {
      const steps = this.tasks.stepsOf(task)
      const fromTerminal = stageOfTerminal(steps, step, input.terminal)
      if (fromTerminal !== undefined) {
        return fromTerminal
      }
      const others = steps
        .flatMap(stagesOf)
        .filter((stage) => terminalOfStage(steps, stage.id) === input.terminal)
      if (others.length > 0) {
        const what =
          others.length === 1
            ? `The ${others[0]?.id} stage of Shell ${input.terminal} does not run now`
            : `No stage of Shell ${input.terminal} runs now`
        throw new StageActionError(`${what}: ${task.id} is at ${task.stage}. Stop here.`)
      }
    }
    if (isStage(step)) {
      return step
    }
    const fitting = stages.filter((stage) => states.includes(laneState(task, step, stage.id)))
    if (fitting.length === 1 && fitting[0] !== undefined) {
      return fitting[0]
    }
    throw new StageActionError(
      `${task.id} runs ${stages.map((stage) => stage.id).join(' and ')} side by side. Name the stage`,
    )
  }

  async act(taskId: string, action: StageAction, input: StageActionInput = {}): Promise<Task> {
    const task = this.tasks.getTask(taskId)
    const step = this.stepOf(task)
    const source = input.byAgent === true ? 'agent' : 'human'
    if (action === 'start') {
      this.tasks.setWithoutAgent(taskId, false)
      await this.startLanes(taskId, ['draft', 'ready'])
      return this.tasks.getTask(taskId)
    }
    if (action === 'report') {
      const stage = this.resolveStage(task, step, input, ['running', 'needs-input'])
      const reporter = input.terminal === undefined ? source : 'agent'
      await this.reportAndCheck(taskId, stage.id, reporter, input.terminal)
      return this.tasks.getTask(taskId)
    }
    if (action === 'reopen') {
      if (input.to !== undefined) {
        this.checkTarget(task, input.to)
      }
      // A person can type the command in a task shell too; only an agent there counts.
      const byAgent = input.terminal !== undefined && this.agentRuns(taskId, input.terminal)
      const from = byAgent ? this.agentSendsBack(task, step, input.terminal ?? '') : undefined
      this.leaving.set(taskId, step)
      try {
        await this.apply(
          taskId,
          'stage.reopened',
          {
            ...(input.to === undefined ? {} : { to: input.to }),
            ...(input.reason === undefined ? {} : { reason: input.reason }),
            ...(from === undefined ? {} : { from, byAgent: true }),
          },
          source,
        )
      } finally {
        this.leaving.delete(taskId)
      }
      // Its turn ends after this, as after `natamux done`. A closed shell has no turn left.
      if (from !== undefined && this.terminals.list(taskId).includes(input.terminal ?? '')) {
        this.reported.set(`${taskId}/${input.terminal}`, from)
      }
      return this.afterStart(taskId)
    }
    const plan: Record<typeof action, { type: TaskEvent['type']; states: TaskState[] }> = {
      approve: { type: 'stage.approved', states: ['review', 'needs-input'] },
      skip: {
        type: 'stage.skipped',
        states: ['draft', 'ready', 'running', 'needs-input', 'review'],
      },
      pass: { type: 'stage.passed', states: ['ready', 'running', 'review'] },
      fail: { type: 'stage.failed', states: ['running'] },
    }
    const { type, states } = plan[action]
    const stage = this.resolveStage(task, step, input, states)
    await this.apply(
      taskId,
      type,
      { stage: stage.id, ...(input.reason === undefined ? {} : { reason: input.reason }) },
      source,
    )
    return this.afterStart(taskId)
  }

  // A send back goes to this stage or one before it, on the way the task went.
  private checkTarget(task: Task, to: string): void {
    const steps = this.tasks.stepsOf(task)
    if (isNotTaken(steps, task.paths, to)) {
      throw new StageInputError(`${to} is on a path this task did not take`)
    }
    const target = stepIndex(steps, to)
    const at = stepIndex(steps, task.stage)
    // Stages of one path share the index of their branch.
    const later = (pathOf(steps, to)?.index ?? 0) > (pathOf(steps, task.stage)?.index ?? 0)
    if (target < 0 || target > at || (target === at && later)) {
      throw new StageInputError(
        `${task.id} has no stage "${to}" to go back to. Name this stage or one before it`,
      )
    }
  }

  // An agent that reviews sends the task back by itself, at most MAX_AGENT_SEND_BACKS times from
  // its stage; then the stage waits for you, so two agents that disagree do not loop forever.
  private agentSendsBack(task: Task, step: Step, terminal: string): string {
    const from = stageOfTerminal(this.tasks.stepsOf(task), step, terminal)?.id ?? task.stage
    const before = this.sendBacksFrom(task.id, from)
    if (before >= MAX_AGENT_SEND_BACKS) {
      const name = stageName({ id: from })
      this.tasks.applyEvent(task.id, {
        source: 'agent',
        type: 'agent.asked',
        payload: {
          stage: from,
          message: `${name} sent the task back ${before} times. Decide what to do.`,
        },
      })
      throw new StageActionError(
        `${task.id} went back ${before} times already, so it waits for the user now. Stop here.`,
      )
    }
    return from
  }

  private sendBacksFrom(taskId: string, from: string): number {
    return this.tasks
      .listEvents(taskId)
      .filter(
        ({ event }) =>
          event.type === 'stage.reopened' &&
          event.payload.byAgent === true &&
          event.payload.from === from,
      ).length
  }

  private async apply(
    taskId: string,
    type: TaskEvent['type'],
    payload: Record<string, unknown>,
    source: TaskEvent['source'],
  ): Promise<void> {
    const result = await this.applyEnding(taskId, { source, type, payload })
    if (!result.accepted) {
      throw new StageActionError(`${taskId} cannot take "${type}" now (${result.task.state})`)
    }
  }

  // A move into a branch takes the first path whose condition holds. The conditions are checked
  // here, before the move, and the event keeps the choice, so a replay of the log never runs them.
  async withPath(taskId: string, input: EventInput): Promise<EventInput> {
    const task = this.tasks.getTask(taskId)
    const steps = this.tasks.stepsOf(task)
    const position = { stage: task.stage, state: task.state, lanes: task.lanes, paths: task.paths }
    const at = new Date(this.now()).toISOString()
    const event = completeEvent(position, { ...input, taskId, at }, steps)
    const branch = entersBranch(position, event, steps)
    if (branch === undefined) {
      const { path: _path, because: _because, ...payload } = input.payload
      return { ...input, payload }
    }
    // The conditions and their order come from the settings now, as names and gates do. A path
    // whose stages are all off now still runs for a task that has it, so it is checked too.
    const current = this.tasks.settingsOf(task.repo).stages.find((step) => step.id === branch.id)
    const folder = workingDir(this.dataDir, task)
    const picked = await pickPath(
      current !== undefined && isBranch(current) ? current : branch,
      { folder, cwd: task.workspace ?? folder, paths: branch.paths.map((path) => path.id) },
      this.runCommand,
    )
    // Only the daemon picks the path. The stage stays the one the event was for, though the
    // conditions took a while.
    const { path: _path, because: _because, ...payload } = event.payload
    const path = picked.path === undefined ? {} : { path: picked.path }
    return { ...input, payload: { ...payload, ...path, because: picked.because } }
  }

  // An event that may end a stage, with the path it takes at a branch it enters.
  private async applyEnding(taskId: string, input: EventInput): Promise<ApplyEventResult> {
    return this.tasks.applyEvent(taskId, await this.withPath(taskId, input))
  }

  // The task once the stages the action moved it to have started, so the answer shows a stage
  // that failed at once, not the ready state before it.
  private async afterStart(taskId: string): Promise<Task> {
    // A skip of a marked stage moves the task on, and the next step starts in work of its own.
    for (let work = this.startWork.get(taskId); work !== undefined; ) {
      await work
      const next = this.startWork.get(taskId)
      work = next === work ? undefined : next
    }
    return this.tasks.getTask(taskId)
  }

  // The report that a stage is done, then its checks.
  private async reportAndCheck(
    taskId: string,
    stage: string,
    source: TaskEvent['source'],
    terminal?: string,
  ): Promise<void> {
    const reported = this.tasks.applyEvent(taskId, {
      source,
      type: 'stage.reported',
      payload: { stage },
    })
    if (!reported.accepted) {
      throw new StageActionError(`${taskId}: the ${stage} stage is not running`)
    }
    if (source === 'agent' && terminal !== undefined) {
      this.reported.set(`${taskId}/${terminal}`, stage)
    }
    await this.check(taskId, stage)
  }

  private lastStart(taskId: string, stage: string) {
    const events = this.tasks.listEvents(taskId)
    for (let index = events.length - 1; index >= 0; index -= 1) {
      const stored = events[index]
      if (stored?.event.type === 'stage.started' && stored.event.payload.stage === stage) {
        const failures = events
          .slice(index)
          .filter(
            (later) =>
              later.event.type === 'stage.check-failed' && later.event.payload.stage === stage,
          ).length
        const head = stored.event.payload.head
        return {
          seq: stored.seq,
          at: stored.event.at,
          head: typeof head === 'string' ? head : null,
          failures,
        }
      }
    }
    return { seq: -1, at: null, head: null, failures: 0 }
  }

  private async check(taskId: string, stageId: string): Promise<void> {
    const key = `${taskId}/${stageId}/${this.lastStart(taskId, stageId).seq}`
    if (this.checking.has(key)) {
      return
    }
    this.checking.add(key)
    try {
      await this.checkOnce(taskId, stageId)
    } finally {
      this.checking.delete(key)
    }
  }

  private async checkOnce(taskId: string, stageId: string): Promise<void> {
    const task = this.tasks.getTask(taskId)
    const step = this.stepOf(task)
    const stage = stagesOf(step).find((candidate) => candidate.id === stageId)
    if (stage === undefined) {
      return
    }
    if (stage.kind === 'context') {
      this.addAttachmentSection(task, stage.output ?? 'context.md')
    }
    const start = this.lastStart(taskId, stageId)
    const folder = workingDir(this.dataDir, task)
    const result = await runStageChecks(
      {
        stage,
        folder,
        cwd: task.workspace ?? folder,
        isWorktree: task.workspace !== null,
        startedAt: start.at,
        startHead: start.head,
      },
      this.runCommand,
    ).catch((error: unknown) => ({
      passed: false,
      reasons: [error instanceof Error ? error.message : String(error)],
      passes: [],
    }))
    // The stage started again while the checks ran: their result is about the round before.
    if (this.lastStart(taskId, stageId).seq !== start.seq) {
      return
    }
    if (result.passed && this.waitsForCi(task, stage)) {
      if (this.ciRounds.has(`${taskId}/${stageId}`)) {
        return
      }
      this.tasks.recordEvent(taskId, 'stage.ci-started', {
        stage: stageId,
        branch: ciBranchOf(task.branch ?? ''),
      })
      this.startCiRound(taskId, stageId, start.seq, result.passes)
      return
    }
    if (result.passed) {
      await this.applyChecked(taskId, stageId, start.seq, result.passes)
      return
    }
    this.failChecks(taskId, stageId, result.reasons)
  }

  // The conditions of a branch after the stage can take minutes, and the stage may start again
  // meanwhile: then the checks are about the round before.
  private async applyChecked(taskId: string, stageId: string, seq: number, passes: string[]) {
    const event = await this.withPath(taskId, {
      source: 'system',
      type: 'stage.checked',
      payload: { stage: stageId, passes },
    })
    if (this.lastStart(taskId, stageId).seq === seq) {
      this.tasks.applyEvent(taskId, event)
    }
  }

  // The task asked for CI on this stage, and has the branch to push.
  private waitsForCi(task: Task, stage: Stage): boolean {
    const chosen = (task.ciStages ?? []).includes(stage.id)
    return chosen && offersCi(stage) && task.workspace !== null && task.branch !== null
  }

  private startCiRound(taskId: string, stageId: string, seq: number, passes: string[]): void {
    const key = `${taskId}/${stageId}`
    if (this.ciRounds.has(key)) {
      return
    }
    this.ciRounds.add(key)
    const round = this.ciRound(taskId, stageId, seq, passes)
    this.background(round.finally(() => this.ciRounds.delete(key)))
  }

  // CI takes minutes, so it runs after the report has its answer. Its result counts only for the
  // round of the stage that asked for it.
  private async ciRound(taskId: string, stageId: string, seq: number, passes: string[]) {
    const { workspace, branch } = this.tasks.getTask(taskId)
    // The workspace can be removed while CI waits, then the daemon restarts. An empty worktree
    // would run git in the daemon's own folder and push that checkout.
    const ci =
      workspace === null || branch === null
        ? ciError('The task has no workspace or branch for the CI check.')
        : await this.checkCi({ worktree: workspace, branch }).catch(ciError)
    if (this.lastStart(taskId, stageId).seq !== seq) {
      return
    }
    if (ci.passed) {
      await this.applyChecked(taskId, stageId, seq, [...passes, ...ci.passes])
      return
    }
    if (!ci.final && this.sendBackForCi(taskId, stageId, ci.reasons, ci.logs)) {
      return
    }
    this.failChecks(taskId, stageId, ci.reasons, ci.final, ci.logs)
  }

  // A CI failure goes back to an agent as a send back, at most MAX_AGENT_SEND_BACKS times, so the
  // stage starts a new round and its prompt is pasted as a review's would be. An agent stage gets
  // it with the log. A testing stage only runs the code, so its failure goes to the stage that
  // changes code, without the log.
  private sendBackForCi(
    taskId: string,
    stageId: string,
    reasons: string[],
    logs = reasons,
  ): boolean {
    const task = this.tasks.getTask(taskId)
    const steps = this.tasks.stepsOf(task)
    const stage = stagesOf(this.stepOf(task)).find((candidate) => candidate.id === stageId)
    const own = stage?.kind === 'agent'
    const to = own
      ? stage
      : stage?.kind === 'testing'
        ? codeStageBefore(steps, stageId, task.paths)
        : undefined
    if (stage === undefined || to === undefined) {
      return false
    }
    if (this.sendBacksFrom(taskId, stageId) >= MAX_AGENT_SEND_BACKS) {
      this.failChecks(taskId, stageId, reasons, true)
      return true
    }
    // Ends the CI wait of the stage, so a restart does not check it again.
    this.tasks.applyEvent(taskId, {
      source: 'system',
      type: 'stage.check-failed',
      payload: { stage: stageId, reasons, final: false },
    })
    // The stage card shows the lines before the first blank line.
    const reason = [
      `CI failed on ${stageName(stage)}:`,
      ...reasons.map((each) => `- ${each}`),
      '',
      ...(own ? logs.map((log) => plainText(log).trimEnd()) : []),
      'Read the failed log with `gh run view <run id> --log-failed`; the id ends the run url.',
    ].join('\n')
    this.leaving.set(taskId, this.stepOf(task))
    try {
      this.tasks.applyEvent(taskId, {
        source: 'system',
        type: 'stage.reopened',
        payload: { to: to.id, reason, from: stageId, byAgent: true, ci: true },
      })
    } finally {
      this.leaving.delete(taskId)
    }
    return true
  }

  // An agent gets one more try; a stage that no agent works on, or a failure no agent can fix,
  // waits for you at once.
  private failChecks(
    taskId: string,
    stageId: string,
    reasons: string[],
    cannotFix = false,
    // What the agent reads, when it is longer than the reasons the stage card shows.
    logs = reasons,
  ) {
    const task = this.tasks.getTask(taskId)
    const step = this.stepOf(task)
    const stage = stagesOf(step).find((candidate) => candidate.id === stageId)
    if (stage === undefined) {
      return
    }
    const start = this.lastStart(taskId, stageId)
    const terminal = terminalOfStage(this.tasks.stepsOf(task), stageId)
    const agentWorks =
      (stage.kind === 'agent' || stage.kind === 'testing' || this.isAgentContext(task)) &&
      this.agentRuns(taskId, terminal)
    const final = cannotFix || !agentWorks || start.failures >= 1
    this.tasks.applyEvent(taskId, {
      source: 'system',
      type: 'stage.check-failed',
      payload: { stage: stageId, reasons, final },
    })
    if (!final) {
      const text = [
        'The checks of this stage failed:',
        ...logs.map((reason) => `- ${reason}`),
        'Fix them, then run `natamux done` again.',
      ].join('\n')
      try {
        this.terminals.paste(taskId, text, terminal)
      } catch {
        // The terminal closed since: the reasons are in the log, and the next failure waits.
      }
    }
  }

  // An agent that read the context from a skill or an MCP server may leave the attachments out of
  // it. The later stages find them only through the context.
  private addAttachmentSection(task: Task, output: string): void {
    const attachments = listAttachments(this.dataDir, task.id)
    const text = attachments.length === 0 ? undefined : readStageFile(this.dataDir, task, output)
    if (text === undefined) {
      return
    }
    const next = withAttachmentSection(text, attachments)
    if (next !== text) {
      writeStageFile(this.dataDir, task, output, next)
    }
  }

  private isAgentContext(task: Task): boolean {
    const type = this.sourceOf(task).type
    return type === 'skill' || type === 'mcp'
  }
}
