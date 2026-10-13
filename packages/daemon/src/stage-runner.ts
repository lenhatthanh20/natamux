import { mkdirSync, writeFileSync } from 'node:fs'
import { homedir } from 'node:os'
import { basename, join } from 'node:path'
import {
  type ContextSource,
  codeStageBefore,
  findStep,
  isGroup,
  offersCi,
  RECORD_ONLY_EVENT_TYPES,
  runsWorkflow,
  type Stage,
  type Step,
  stageAgent,
  stageName,
  stageOfTerminal,
  stagesOf,
  stepIndex,
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
import type { DaemonMessage, EventBus } from './event-bus.js'
import { deleteRemoteRef, type RunGh, runGh } from './git.js'
import { currentHead, type RunCommand, runShellCommand, runStageChecks } from './stage-checks.js'
import {
  archiveStageFiles,
  excludeStageDir,
  readStageFile,
  restoreStageFiles,
  workingDir,
  writeStageFile,
} from './stage-files.js'
import { contextPrompt, manualContext, stagePrompt } from './stage-prompts.js'
import type { TaskService } from './task-service.js'
import { MAIN_TERMINAL, SHELLS, type TerminalService } from './terminal-service.js'

// After the launch line is typed, the shell stays in the foreground until the agent is up.
export const LAUNCH_GRACE_MS = 30_000

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

function laneState(task: Pick<Task, 'state' | 'lanes'>, step: Step, stageId: string): TaskState {
  return isGroup(step) ? (task.lanes[stageId] ?? 'done') : task.state
}

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
  // Tasks made for a plain shell. Their agent stages wait for a person to start an agent.
  private readonly withoutAgent = new Set<string>()
  // The terminal where each task runs its stage command.
  private readonly commandTerminals = new Map<string, string>()
  // The stages each task is starting now, so an action can answer with the task after them.
  private readonly startWork = new Map<string, Promise<void>>()
  private readonly starting = new Set<string>()
  private readonly pending = new Set<Promise<unknown>>()
  // Finished tasks whose CI branch was deleted, so later updates do not push again.
  private readonly ciCleaned = new Set<string>()
  // The CI checks that run now, by task and stage. A second report waits for the first check.
  private readonly ciRounds = new Set<string>()
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
    const work = this.startLanes(taskId, states, cause).catch(() => undefined)
    this.startWork.set(taskId, work)
    this.background(
      work.finally(() => {
        if (this.startWork.get(taskId) === work) {
          this.startWork.delete(taskId)
        }
      }),
    )
  }

  private stepOf(task: Task): Step {
    const step = findStep(this.tasks.stepsOf(task), task.stage)
    if (step === undefined) {
      throw new StageActionError(`${task.id} is at an unknown stage "${task.stage}"`)
    }
    return step
  }

  // The stage a hook from this terminal is about, and its state. Null for a shell that no stage
  // uses, where the user started an agent by hand.
  laneFor(task: Task, terminal: string | undefined): { stage: string; state: TaskState } | null {
    const step = this.stepOf(task)
    const stage = stageOfTerminal(step, terminal ?? MAIN_TERMINAL)
    return stage === undefined ? null : { stage: stage.id, state: laneState(task, step, stage.id) }
  }

  // The stage an agent in this terminal works on, and its own state, for the dashboard.
  laneOfTerminal(task: Task, terminal: string): TerminalLane {
    return terminalLane(task, findStep(this.tasks.stepsOf(task), task.stage), terminal)
  }

  // True once for the turn end that follows a report from this terminal.
  takeReport(taskId: string, terminal: string): boolean {
    return this.reported.delete(`${taskId}/${terminal}`)
  }

  private onMessage(message: DaemonMessage): void {
    if (message.kind === 'task.deleted') {
      this.finish(message.taskId)
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
    if (['stage.approved', 'stage.passed', 'stage.checked'].includes(event.type)) {
      archiveStageFiles(this.dataDir, task)
    }
    const finished = task.state === 'done' || task.state === 'cancelled'
    const usedCi = (task.ciStages ?? []).length > 0 && !this.ciCleaned.has(task.id)
    if (finished && usedCi && task.branch !== null) {
      this.ciCleaned.add(task.id)
      // The CI branch only held snapshots for checks. Gone already, or offline: nothing to do.
      const ref = ciBranchOf(task.branch)
      this.background(deleteRemoteRef(this.repoPath(task.repo), ref).catch(() => undefined))
    }
    if (event.type === 'workspace.created' && task.workspace !== null) {
      restoreStageFiles(this.dataDir, task)
      restoreAttachments(this.dataDir, task)
      this.background(excludeStageDir(task.workspace))
      if (this.waitingForWorkspace.delete(task.id)) {
        this.startInBackground(task.id, ['ready', 'draft'])
      }
    }
    if (message.moved === true) {
      this.startInBackground(task.id, ['ready'], event)
    } else if (event.type === 'agent.resumed' && event.source !== 'agent') {
      // After this message has reached every listener, so its events come in order.
      this.background(Promise.resolve().then(() => this.resumeStage(task.id, event)))
    }
    const ended = task.state === 'done' || task.state === 'cancelled'
    if (ended && !RECORD_ONLY_EVENT_TYPES.includes(event.type)) {
      this.finish(task.id)
    }
    if (task.state === 'done' || event.type === 'task.edited') {
      this.startWaiting()
    }
  }

  // A task that is done or cancelled needs its agent and its shells no more.
  private finish(taskId: string): void {
    this.terminals.close(taskId)
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
    this.withoutAgent.delete(taskId)
  }

  // A task made with no agent: its stages that need one stay ready, and no agent starts in its
  // shell until a person starts one.
  leaveToPerson(taskId: string): void {
    this.withoutAgent.add(taskId)
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
    const task = this.tasks.getTask(taskId)
    const step = this.stepOf(task)
    for (const stage of stagesOf(step)) {
      if (states.includes(laneState(task, step, stage.id))) {
        await this.startStage(task.id, step, stage, cause)
      }
    }
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
            await this.startCommand(task, step, stage, stage.command)
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

  private markStarted(
    task: Task,
    step: Step,
    stage: Stage,
    payload: Record<string, unknown>,
  ): boolean {
    const started = this.tasks.applyEvent(task.id, {
      source: 'system',
      type: 'stage.started',
      payload: { stage: stage.id, ...payload },
    }).accepted
    // The turn end of an earlier report of this stage never came; it must not swallow one of
    // this round.
    const key = `${task.id}/${terminalOfStage(step, stage.id)}`
    if (started && this.reported.get(key) === stage.id) {
      this.reported.delete(key)
    }
    return started
  }

  private skillContext(task: Task, agent: string): SkillContext {
    return { agent, workspace: task.workspace ?? this.repoPath(task.repo), home: this.home }
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
    if (this.withoutAgent.has(task.id) && !this.chosenAgents.has(task.id)) {
      return
    }
    await excludeStageDir(task.workspace).catch(() => undefined)
    const head = await currentHead(task.workspace)
    const own = stageAgent(step, stage)
    const agent = own.agent ?? this.chosenAgents.get(task.id) ?? task.agent ?? 'claude-code'
    const terminal = terminalOfStage(step, stage.id)
    // A prompt typed to the agent took the stage up again: the agent has its prompt. With no
    // agent left (the app restarted), Claude Code goes on with its last session of the stage.
    const running = resumed && this.agentRuns(task.id, terminal)
    const session = resumed && !running ? this.lastSession(task.id, stage.id, agent) : undefined
    const prompt = running || session !== undefined ? undefined : promptFor(agent)
    // The stage card says which skills of the prompt the agent did not get, and whether it asked
    // for a workflow.
    const missing = prompt?.missing.length ? { missingSkills: prompt.missing, agent } : {}
    const workflow = prompt?.workflow === true ? { workflow: true } : {}
    if (!this.markStarted(task, step, stage, { head, ...missing, ...workflow }) || running) {
      return
    }
    this.launch(task.id, stage.id, agent, terminal, prompt?.text ?? '', session, own)
  }

  // A person resumed a stage that waited for them. Its agent may be gone, for example after a
  // restart, and then starts again.
  private resumeStage(taskId: string, event: TaskEvent): void {
    const task = this.tasks.getTask(taskId)
    const step = this.stepOf(task)
    const stage = stagesOf(step).find((candidate) => candidate.id === event.payload.stage)
    if (stage !== undefined) {
      this.relaunch(task, step, stage)
    }
  }

  // Agents end with the daemon. At its start, each stage an agent worked on gets its agent back,
  // and the task keeps its state.
  resumeAgents(): void {
    for (const task of this.tasks.listTasks()) {
      if (task.state === 'done' || task.state === 'cancelled') {
        continue
      }
      try {
        const step = this.stepOf(task)
        for (const stage of stagesOf(step)) {
          const working = ['running', 'needs-input'].includes(laneState(task, step, stage.id))
          // No agent Natamux started for it: a person runs their own in the shell.
          const agent = stageAgent(step, stage).agent ?? task.agent
          if (working && agent !== null && isLaunchable(agent)) {
            // In a group, the newest session of the folder may be the other stage's.
            this.relaunch(task, step, stage, !isGroup(step))
          }
        }
      } catch {
        // A stage the settings no longer have: the task waits for a person, the daemon starts.
      }
    }
  }

  // Starts the agent of a stage again when it is gone. Claude Code goes on with its session of the
  // stage, or with `newest` the newest of the folder; other agents get the stage prompt again.
  private relaunch(task: Task, step: Step, stage: Stage, newest = false): void {
    const byAgent =
      stage.kind === 'agent' || (stage.kind === 'testing' && stage.command === undefined)
    if (!byAgent || task.workspace === null) {
      return
    }
    const terminal = terminalOfStage(step, stage.id)
    if (this.agentRuns(task.id, terminal)) {
      return
    }
    const own = stageAgent(step, stage)
    const agent = own.agent ?? task.agent ?? 'claude-code'
    const session =
      this.lastSession(task.id, stage.id, agent) ??
      (newest && agent === 'claude-code' ? LAST_SESSION : undefined)
    const text = session === undefined ? this.promptOf(task, stage)(agent).text : ''
    this.launch(task.id, stage.id, agent, terminal, text, session, own)
  }

  // The last session that Claude Code reported from this stage. Other agents start new ones.
  private lastSession(taskId: string, stageId: string, agent: string): string | undefined {
    if (agent !== 'claude-code') {
      return undefined
    }
    for (const { event } of [...this.tasks.listEvents(taskId)].reverse()) {
      // A session from before another agent was started is not this agent's. The own agent of
      // another stage runs in another shell, so it does not count.
      const otherStage = event.payload.stage !== undefined && event.payload.stage !== stageId
      if (event.type === 'agent.launched' && event.payload.agent !== agent && !otherStage) {
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
    this.withoutAgent.delete(taskId)
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
    const programOf = (agent: LaunchableAgent) => this.agents.programOf(agent)
    for (const line of commands ?? []) {
      const agent = agentOfCommand(line, programOf)
      if (agent !== undefined) {
        return agent
      }
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
  onOutput(taskId: string, terminal: string, data: string): void {
    const key = `${taskId}/${terminal}`
    if (this.platform !== 'win32' || !this.launched.has(key)) {
      return
    }
    const text = (this.outputTails.get(key) ?? '') + data
    if (AGENT_EXITED_MARKS.some((mark) => text.includes(mark))) {
      this.launched.delete(key)
      this.outputTails.delete(key)
      return
    }
    this.outputTails.set(key, text.slice(-MARK_TAIL_CHARS))
  }

  // A command whose exit code decides the stage, such as the test command.
  private async startCommand(task: Task, step: Step, stage: Stage, command: string) {
    if (task.workspace === null) {
      this.waitingForWorkspace.add(task.id)
      return
    }
    if (!this.markStarted(task, step, stage, {})) {
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
    if (!this.markStarted(task, step, stage, { source: source.type })) {
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
      this.tasks.applyEvent(task.id, {
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
  // the step that can take the action.
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
    if (!isGroup(step)) {
      return step
    }
    if (input.terminal !== undefined) {
      const fromTerminal = stageOfTerminal(step, input.terminal)
      if (fromTerminal !== undefined) {
        return fromTerminal
      }
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
    if (action === 'start') {
      this.withoutAgent.delete(taskId)
      await this.startLanes(taskId, ['draft', 'ready'])
      return this.tasks.getTask(taskId)
    }
    if (action === 'report') {
      const stage = this.resolveStage(task, step, input, ['running', 'needs-input'])
      const source = input.terminal === undefined ? 'human' : 'agent'
      await this.reportAndCheck(taskId, stage.id, source, input.terminal)
      return this.tasks.getTask(taskId)
    }
    if (action === 'reopen') {
      if (input.to !== undefined) {
        this.checkTarget(task, input.to)
      }
      // A person can type the command in a task shell too; only an agent there counts.
      const byAgent = input.terminal !== undefined && this.agentRuns(taskId, input.terminal)
      const from = byAgent ? this.agentSendsBack(task, step, input.terminal ?? '') : undefined
      this.apply(taskId, 'stage.reopened', {
        ...(input.to === undefined ? {} : { to: input.to }),
        ...(input.reason === undefined ? {} : { reason: input.reason }),
        ...(from === undefined ? {} : { from, byAgent: true }),
      })
      if (from !== undefined) {
        // Its turn ends after this, as after `natamux done`.
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
    this.apply(taskId, type, {
      stage: stage.id,
      ...(input.reason === undefined ? {} : { reason: input.reason }),
    })
    return this.afterStart(taskId)
  }

  // A send back goes to this stage or one before it.
  private checkTarget(task: Task, to: string): void {
    const steps = this.tasks.stepsOf(task)
    const at = stepIndex(steps, to)
    if (at < 0 || at > stepIndex(steps, task.stage)) {
      throw new StageInputError(
        `${task.id} has no stage "${to}" to go back to. Name this stage or one before it`,
      )
    }
  }

  // An agent that reviews sends the task back by itself, at most MAX_AGENT_SEND_BACKS times from
  // its stage; then the stage waits for you, so two agents that disagree do not loop forever.
  private agentSendsBack(task: Task, step: Step, terminal: string): string {
    const from = stageOfTerminal(step, terminal)?.id ?? task.stage
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

  private apply(taskId: string, type: TaskEvent['type'], payload: Record<string, unknown>): void {
    const result = this.tasks.applyEvent(taskId, { source: 'human', type, payload })
    if (!result.accepted) {
      throw new StageActionError(`${taskId} cannot take "${type}" now (${result.task.state})`)
    }
  }

  // The task once the stages the action moved it to have started, so the answer shows a stage
  // that failed at once, not the ready state before it.
  private async afterStart(taskId: string): Promise<Task> {
    await this.startWork.get(taskId)
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
      this.tasks.applyEvent(taskId, {
        source: 'system',
        type: 'stage.checked',
        payload: { stage: stageId, passes: result.passes },
      })
      return
    }
    this.failChecks(taskId, stageId, result.reasons)
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
    const task = this.tasks.getTask(taskId)
    const input = { worktree: task.workspace ?? '', branch: task.branch ?? '' }
    const ci = await this.checkCi(input).catch(ciError)
    if (this.lastStart(taskId, stageId).seq !== seq) {
      return
    }
    if (ci.passed) {
      this.tasks.applyEvent(taskId, {
        source: 'system',
        type: 'stage.checked',
        payload: { stage: stageId, passes: [...passes, ...ci.passes] },
      })
      return
    }
    if (!ci.final && this.sendBackForCi(taskId, stageId, ci.reasons)) {
      return
    }
    this.failChecks(taskId, stageId, ci.reasons, ci.final, ci.logs)
  }

  // A testing stage only runs the code, so its CI failure goes back to the stage that changes
  // code, at most MAX_AGENT_SEND_BACKS times, as an agent's send back.
  private sendBackForCi(taskId: string, stageId: string, reasons: string[]): boolean {
    const task = this.tasks.getTask(taskId)
    const steps = this.tasks.stepsOf(task)
    const stage = stagesOf(this.stepOf(task)).find((candidate) => candidate.id === stageId)
    const to = codeStageBefore(steps, stageId)
    if (stage?.kind !== 'testing' || to === undefined) {
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
    const reason = [
      `CI failed on ${stageName(stage)}:`,
      ...reasons.map((each) => `- ${each}`),
      'Read the failed log with `gh run view <run id> --log-failed`; the id ends the run url.',
    ].join('\n')
    this.tasks.applyEvent(taskId, {
      source: 'system',
      type: 'stage.reopened',
      payload: { to: to.id, reason, from: stageId, byAgent: true },
    })
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
    const terminal = terminalOfStage(step, stageId)
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
