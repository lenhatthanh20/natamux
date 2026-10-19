import {
  isNotTaken,
  isStage,
  pathOf,
  type Stage,
  type Step,
  stageName,
  stageOfTerminal,
  stagesOf,
  stepIndex,
  type Task,
  type TaskEvent,
  type TaskState,
  terminalOfStage,
} from '@natamux/core'
import {
  laneState,
  MAX_AGENT_SEND_BACKS,
  type StageAction,
  StageActionError,
  type StageActionInput,
  StageInputError,
} from './stage-actions.js'
import type { StageRun } from './stage-run.js'

// The actions that move a stage on by a plan of their own.
type Move = Exclude<StageAction, 'start' | 'report' | 'reopen'>

// The actions a person or an agent takes on the stages of a task: start, report, send back,
// approve, skip, pass and fail.
export class StageAct {
  constructor(private readonly run: StageRun) {}

  async act(taskId: string, action: StageAction, input: StageActionInput = {}): Promise<Task> {
    const task = this.run.tasks.getTask(taskId)
    const step = this.run.stepOf(task)
    const source = input.byAgent === true ? 'agent' : 'human'
    if (action === 'start') {
      return this.start(taskId)
    }
    if (action === 'report') {
      return this.report(task, step, input, source)
    }
    if (action === 'reopen') {
      return this.reopen(task, step, input, source)
    }
    return this.move(task, step, action, input, source)
  }

  private async start(taskId: string): Promise<Task> {
    this.run.tasks.setWithoutAgent(taskId, false)
    await this.run.startLanes(taskId, ['draft', 'ready'])
    return this.run.tasks.getTask(taskId)
  }

  private async report(
    task: Task,
    step: Step,
    input: StageActionInput,
    source: TaskEvent['source'],
  ): Promise<Task> {
    const stage = this.resolveStage(task, step, input, ['running', 'needs-input'])
    const reporter = input.terminal === undefined ? source : 'agent'
    await this.run.gate.reportAndCheck(task.id, stage.id, reporter, input.terminal)
    return this.run.tasks.getTask(task.id)
  }

  private async reopen(
    task: Task,
    step: Step,
    input: StageActionInput,
    source: TaskEvent['source'],
  ): Promise<Task> {
    const taskId = task.id
    if (input.to !== undefined) {
      this.checkTarget(task, input.to)
    }
    // A person can type the command in a task shell too; only an agent there counts.
    const byAgent =
      input.terminal !== undefined && this.run.shells.agentRuns(taskId, input.terminal)
    const from = byAgent ? this.agentSendsBack(task, step, input.terminal ?? '') : undefined
    this.run.leaving.set(taskId, step)
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
      this.run.leaving.delete(taskId)
    }
    // Its turn ends after this, as after `natamux done`. A closed shell has no turn left.
    if (from !== undefined && this.run.terminals.list(taskId).includes(input.terminal ?? '')) {
      this.run.reported.set(`${taskId}/${input.terminal}`, from)
    }
    return this.afterStart(taskId)
  }

  private async move(
    task: Task,
    step: Step,
    action: Move,
    input: StageActionInput,
    source: TaskEvent['source'],
  ): Promise<Task> {
    const plan: Record<Move, { type: TaskEvent['type']; states: TaskState[] }> = {
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
      task.id,
      type,
      { stage: stage.id, ...(input.reason === undefined ? {} : { reason: input.reason }) },
      source,
    )
    return this.afterStart(task.id)
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
      const steps = this.run.tasks.stepsOf(task)
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

  // A send back goes to this stage or one before it, on the way the task went.
  private checkTarget(task: Task, to: string): void {
    const steps = this.run.tasks.stepsOf(task)
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
    const from = stageOfTerminal(this.run.tasks.stepsOf(task), step, terminal)?.id ?? task.stage
    const before = this.sendBacksFrom(task.id, from)
    if (before >= MAX_AGENT_SEND_BACKS) {
      const name = stageName({ id: from })
      this.run.tasks.applyEvent(task.id, {
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

  sendBacksFrom(taskId: string, from: string): number {
    return this.run.tasks
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
    const result = await this.run.applyEnding(taskId, { source, type, payload })
    if (!result.accepted) {
      throw new StageActionError(`${taskId} cannot take "${type}" now (${result.task.state})`)
    }
  }

  // The task once the stages the action moved it to have started, so the answer shows a stage
  // that failed at once, not the ready state before it.
  private async afterStart(taskId: string): Promise<Task> {
    // A skip of a marked stage moves the task on, and the next step starts in work of its own.
    for (let work = this.run.startWork.get(taskId); work !== undefined; ) {
      await work
      const next = this.run.startWork.get(taskId)
      work = next === work ? undefined : next
    }
    return this.run.tasks.getTask(taskId)
  }
}
