import {
  type ContextSource,
  findStep,
  isGroup,
  type Stage,
  type Step,
  stagesOf,
  type Task,
  type TaskEvent,
  type TaskState,
} from '@helmsman/core'
import type { AgentRunner } from './agent-runner.js'
import type { DaemonMessage, EventBus } from './event-bus.js'
import { type RunGh, runGh } from './git.js'
import { currentHead, type RunCommand, runShellCommand, runStageChecks } from './stage-checks.js'
import {
  archiveStageFiles,
  excludeStageDir,
  restoreStageFiles,
  workingDir,
  writeStageFile,
} from './stage-files.js'
import { contextPrompt, manualContext, stagePrompt } from './stage-prompts.js'
import type { TaskService } from './task-service.js'
import { MAIN_TERMINAL, type TerminalService } from './terminal-service.js'

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
  // The terminal the action came from (`helmsman done` in an agent's terminal).
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

// The stage of a group that changes code keeps the first terminal (and the agent session of the
// steps before); each other stage gets the next terminal.
function codeStage(step: Step): Stage {
  const stages = stagesOf(step)
  const agents = stages.filter((stage) => stage.kind === 'agent')
  return agents.find((stage) => stage.output === undefined) ?? agents[0] ?? (stages[0] as Stage)
}

export function terminalOfStage(step: Step, stageId: string): string {
  if (!isGroup(step)) {
    return MAIN_TERMINAL
  }
  const main = codeStage(step)
  if (main.id === stageId) {
    return MAIN_TERMINAL
  }
  const others = step.parallel.filter((stage) => stage.id !== main.id)
  return String(others.findIndex((stage) => stage.id === stageId) + 2)
}

export function stageOfTerminal(step: Step, terminal: string): Stage | undefined {
  return stagesOf(step).find((stage) => terminalOfStage(step, stage.id) === terminal)
}

function laneState(task: Pick<Task, 'state' | 'lanes'>, step: Step, stageId: string): TaskState {
  return isGroup(step) ? (task.lanes[stageId] ?? 'done') : task.state
}

// The issue number in "#42", "42" or an issue address.
export function issueNumber(ref: string): number | undefined {
  const match = /(?:^#?|\/issues\/)(\d+)\s*$/.exec(ref.trim())
  return match?.[1] === undefined ? undefined : Number(match[1])
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
}

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
  // Stages that reported done; the turn end that follows the report is not a "stopped early".
  private readonly reported = new Set<string>()
  // Tasks whose agent stage waits for a workspace.
  private readonly waitingForWorkspace = new Set<string>()
  // Terminals where this daemon started an agent.
  private readonly launched = new Set<string>()
  private readonly starting = new Set<string>()
  private readonly pending = new Set<Promise<unknown>>()
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
  }

  start(): void {
    this.unsubscribe = this.bus.subscribe((message) => this.onMessage(message))
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

  private stepOf(task: Task): Step {
    const step = findStep(this.tasks.stepsOf(task), task.stage)
    if (step === undefined) {
      throw new StageActionError(`${task.id} is at an unknown stage "${task.stage}"`)
    }
    return step
  }

  // The stage a hook from this terminal is about, and its state.
  laneFor(task: Task, terminal: string | undefined): { stage: string; state: TaskState } {
    const step = this.stepOf(task)
    const stage = isGroup(step)
      ? (stageOfTerminal(step, terminal ?? MAIN_TERMINAL) ?? codeStage(step))
      : step
    return { stage: stage.id, state: laneState(task, step, stage.id) }
  }

  // True once for the turn end that follows a report.
  takeReport(taskId: string, stage: string): boolean {
    return this.reported.delete(`${taskId}/${stage}`)
  }

  private onMessage(message: DaemonMessage): void {
    if (message.kind === 'task.created') {
      const step = this.stepOf(message.task)
      if (stagesOf(step).some((stage) => stage.kind === 'context')) {
        this.background(this.startLanes(message.task.id, ['draft', 'ready']))
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
    if (event.type === 'workspace.created' && task.workspace !== null) {
      restoreStageFiles(this.dataDir, task)
      this.background(excludeStageDir(task.workspace))
      if (this.waitingForWorkspace.delete(task.id)) {
        this.background(this.startLanes(task.id, ['ready', 'draft']))
      }
    }
    if (message.moved === true) {
      this.background(this.startLanes(task.id, ['ready'], event))
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
      const reason =
        cause?.type === 'stage.reopened' && typeof cause.payload.reason === 'string'
          ? cause.payload.reason
          : undefined
      switch (stage.kind) {
        case 'context':
          await this.startContext(task, step, stage)
          return
        case 'agent': {
          const prompt = stagePrompt(stage, task)
          await this.startAgent(
            task,
            step,
            stage,
            reason ? `${prompt}\nWhat to change: ${reason}` : prompt,
          )
          return
        }
        case 'testing':
          if (stage.command === undefined) {
            await this.startAgent(task, step, stage, stagePrompt(stage, task))
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
    return this.tasks.applyEvent(task.id, {
      source: 'system',
      type: 'stage.started',
      payload: { stage: stage.id, ...payload },
    }).accepted
  }

  private async startAgent(task: Task, step: Step, stage: Stage, prompt: string): Promise<void> {
    if (task.workspace === null) {
      this.waitingForWorkspace.add(task.id)
      return
    }
    await excludeStageDir(task.workspace).catch(() => undefined)
    const head = await currentHead(task.workspace)
    if (!this.markStarted(task, stage, { head })) {
      return
    }
    const terminal = terminalOfStage(step, stage.id)
    const key = `${task.id}/${terminal}`
    try {
      const running =
        this.terminals.isOpen(task.id, terminal) &&
        (this.launched.has(key) || (terminal === MAIN_TERMINAL && task.agent !== null))
      if (running) {
        this.terminals.paste(task.id, prompt, terminal)
      } else {
        this.agents.start(task.id, task.agent ?? 'claude-code', prompt, terminal)
        this.launched.add(key)
      }
    } catch (error) {
      this.failStage(task.id, stage.id, error instanceof Error ? error.message : String(error))
    }
  }

  // A command whose exit code decides the stage, such as the test command.
  private async startCommand(task: Task, stage: Stage, command: string): Promise<void> {
    if (task.workspace === null) {
      this.waitingForWorkspace.add(task.id)
      return
    }
    if (!this.markStarted(task, stage, {})) {
      return
    }
    const open = this.terminals.list(task.id).map(Number)
    const terminal = String(Math.max(1, ...open) + 1)
    const id = task.id
    const line = `${command} && helmsman stage pass ${id} --stage ${stage.id} || helmsman stage fail ${id} --stage ${stage.id}`
    try {
      this.terminals.run(task.id, line, terminal)
    } catch (error) {
      this.failStage(task.id, stage.id, error instanceof Error ? error.message : String(error))
    }
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
    const configured = this.tasks
      .settingsOf(task.repo)
      .context.sources.find((source) => source.type === type)
    return configured ?? (type === 'skill' || type === 'mcp' ? { type: 'manual' } : { type })
  }

  private async startContext(task: Task, step: Step, stage: Stage): Promise<void> {
    const source = this.sourceOf(task)
    if (source.type === 'skill' || source.type === 'mcp') {
      await this.startAgent(task, step, stage, contextPrompt(source, task))
      return
    }
    if (!this.markStarted(task, stage, { source: source.type })) {
      return
    }
    const output = stage.output ?? 'context.md'
    try {
      if (source.type === 'github-issue') {
        const text = await this.readIssue(task)
        writeStageFile(this.dataDir, task, output, text)
      } else {
        writeStageFile(this.dataDir, task, output, manualContext(task))
      }
    } catch (error) {
      this.failStage(task.id, stage.id, error instanceof Error ? error.message : String(error))
      return
    }
    this.tasks.recordEvent(task.id, 'context.fetched', { stage: stage.id, source: source.type })
    await this.reportAndCheck(task.id, stage.id, 'system')
  }

  private async readIssue(task: Task): Promise<string> {
    const number =
      issueNumber(task.context?.ref ?? '') ??
      (task.issue?.provider === 'github' ? issueNumber(task.issue.key) : undefined)
    if (number === undefined) {
      throw new StageActionError('Give the issue number, such as #42, to read the context from')
    }
    const output = await this.gh(this.repoPath(task.repo), [
      'issue',
      'view',
      String(number),
      '--json',
      'number,title,body,url,labels,comments',
    ])
    const issue = JSON.parse(output) as GhIssue
    if (task.issue === null && issue.url) {
      this.tasks.linkIssue(
        task.id,
        { provider: 'github', key: `#${issue.number ?? number}`, url: issue.url },
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
      await this.startLanes(taskId, ['draft', 'ready'])
      return this.tasks.getTask(taskId)
    }
    if (action === 'report') {
      const stage = this.resolveStage(task, step, input, ['running', 'needs-input'])
      const source = input.terminal === undefined ? 'human' : 'agent'
      await this.reportAndCheck(taskId, stage.id, source)
      return this.tasks.getTask(taskId)
    }
    if (action === 'reopen') {
      return this.apply(taskId, 'stage.reopened', {
        ...(input.to === undefined ? {} : { to: input.to }),
        ...(input.reason === undefined ? {} : { reason: input.reason }),
      })
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
    return this.apply(taskId, type, {
      stage: stage.id,
      ...(input.reason === undefined ? {} : { reason: input.reason }),
    })
  }

  private apply(taskId: string, type: TaskEvent['type'], payload: Record<string, unknown>): Task {
    const result = this.tasks.applyEvent(taskId, { source: 'human', type, payload })
    if (!result.accepted) {
      throw new StageActionError(`${taskId} cannot take "${type}" now (${result.task.state})`)
    }
    return result.task
  }

  // The report that a stage is done, then its checks.
  private async reportAndCheck(
    taskId: string,
    stage: string,
    source: TaskEvent['source'],
  ): Promise<void> {
    const reported = this.tasks.applyEvent(taskId, {
      source,
      type: 'stage.reported',
      payload: { stage },
    })
    if (!reported.accepted) {
      throw new StageActionError(`${taskId}: the ${stage} stage is not running`)
    }
    if (source === 'agent') {
      this.reported.add(`${taskId}/${stage}`)
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
        return { at: stored.event.at, head: typeof head === 'string' ? head : null, failures }
      }
    }
    return { at: null, head: null, failures: 0 }
  }

  private async check(taskId: string, stageId: string): Promise<void> {
    const task = this.tasks.getTask(taskId)
    const step = this.stepOf(task)
    const stage = stagesOf(step).find((candidate) => candidate.id === stageId)
    if (stage === undefined) {
      return
    }
    const start = this.lastStart(taskId, stageId)
    const result = await runStageChecks(
      {
        stage,
        folder: workingDir(this.dataDir, task),
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
    if (result.passed) {
      this.tasks.applyEvent(taskId, {
        source: 'system',
        type: 'stage.checked',
        payload: { stage: stageId, passes: result.passes },
      })
      return
    }
    // An agent gets one more try; a stage that no agent works on waits for you at once.
    const agentWorks =
      stage.kind === 'agent' || stage.kind === 'testing' || this.isAgentContext(task)
    const final = !agentWorks || start.failures >= 1
    this.tasks.applyEvent(taskId, {
      source: 'system',
      type: 'stage.check-failed',
      payload: { stage: stageId, reasons: result.reasons, final },
    })
    if (!final) {
      const text = [
        'The checks of this stage failed:',
        ...result.reasons.map((reason) => `- ${reason}`),
        'Fix them, then run `helmsman done` again.',
      ].join('\n')
      try {
        this.terminals.paste(taskId, text, terminalOfStage(step, stageId))
      } catch {
        // No terminal: the reasons are in the log, and the next failure waits for you.
      }
    }
  }

  private isAgentContext(task: Task): boolean {
    const type = this.sourceOf(task).type
    return type === 'skill' || type === 'mcp'
  }
}
