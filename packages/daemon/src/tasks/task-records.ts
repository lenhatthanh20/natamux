import {
  applyTaskEvent,
  canRunWorkflow,
  type EventType,
  type IssueLink,
  issueLinkSchema,
  offersCi,
  offersWorkflow,
  type PullRequestStatus,
  pathOf,
  type Step,
  stageName,
  stagesOf,
  stepIndex,
  type Task,
  type TaskEvent,
  type TaskState,
} from '@natamux/core'
import type { EventBus } from '../shared/event-bus.js'
import {
  ACTIVITY_FILE_INTERVAL_MS,
  ACTIVITY_INTERVAL_MS,
  moveTask,
  positionOf,
} from './task-helpers.js'
import {
  type ActivityOptions,
  type ApplyEventResult,
  TaskEditError,
  type WorkspaceChange,
} from './task-inputs.js'
import type { TaskSync } from './task-sync.js'

// What the record writers use of the task service.
type TaskRecordsHost = {
  bus: EventBus
  sync: TaskSync
  getTask(taskId: string): Task
  stepsOf(task: Pick<Task, 'stages' | 'repo'>): Step[]
  applyEvent(taskId: string, rawInput: unknown): ApplyEventResult
  timestamp(): string
}

type Logged = { event: TaskEvent; accepted: boolean }[]

// The events a new status of a pull request logs: only what changed.
function pullRequestEventTypes(
  before: PullRequestStatus | null,
  status: PullRequestStatus,
): EventType[] {
  const types: EventType[] = []
  if (status.checks !== before?.checks && status.checks === 'passing') {
    types.push('ci.passed')
  }
  if (status.checks !== before?.checks && status.checks === 'failing') {
    types.push('ci.failed')
  }
  if (status.state !== before?.state && status.state === 'merged') {
    types.push('pr.merged')
  }
  if (status.state !== before?.state && status.state === 'closed') {
    types.push('pr.closed')
  }
  return types
}

// Applies the events one after the other, so a merge can finish a task in review.
function applyPullRequestEvents(
  task: Task,
  types: readonly EventType[],
  steps: Step[],
  at: string,
): { moving: Task; logged: Logged } {
  let moving = task
  const logged: Logged = []
  for (const type of types) {
    const event: TaskEvent = {
      taskId: task.id,
      source: 'git',
      type,
      at,
      payload: { url: task.pr },
    }
    const applied = applyTaskEvent(positionOf(moving), event, steps)
    moving = moveTask(moving, applied.position, applied.moved, at)
    logged.push({ event, accepted: applied.accepted })
  }
  return { moving, logged }
}

// The writers that log one kind of change of a task, for the other services and the routes.
export class TaskRecords {
  constructor(private readonly host: TaskRecordsHost) {}

  // Records where the task's working copy lives. Called by the workspace service.
  recordWorkspace(taskId: string, change: WorkspaceChange): Task {
    const task = this.host.getTask(taskId)
    const event: TaskEvent = {
      taskId,
      source: 'system',
      type: change.workspace === null ? 'workspace.removed' : 'workspace.created',
      at: this.host.timestamp(),
      payload: { workspace: change.workspace ?? task.workspace, branch: change.branch },
    }
    const updated: Task = {
      ...task,
      workspace: change.workspace,
      branch: change.branch,
      updatedAt: event.at,
    }

    this.host.sync.store(updated, [{ event, accepted: true }])
    this.host.bus.publish({ kind: 'task.updated', task: updated, event, accepted: true })
    return updated
  }

  // Natamux ended the agents of the task, for example with its workspace. A stage whose agent ran
  // or waited for you would stay so with no agent, and would show up in Needs you later.
  stopAgents(taskId: string): Task {
    const task = this.host.getTask(taskId)
    const lanes: [string, TaskState][] =
      Object.keys(task.lanes).length > 0 ? Object.entries(task.lanes) : [[task.stage, task.state]]
    for (const [stage, state] of lanes) {
      if (state === 'running' || state === 'needs-input') {
        this.host.applyEvent(taskId, {
          source: 'system',
          type: 'agent.stopped',
          payload: { stage },
        })
      }
    }
    return this.host.getTask(taskId)
  }

  // Writes down a commit, a push or a new pull request. A pull request also sets `pr` of the task.
  recordGit(
    taskId: string,
    type: 'git.committed' | 'git.pushed' | 'pr.opened',
    payload: Record<string, string>,
  ): Task {
    const task = this.host.getTask(taskId)
    const event: TaskEvent = { taskId, source: 'git', type, at: this.host.timestamp(), payload }
    const updated: Task = {
      ...task,
      pr: type === 'pr.opened' && payload.url ? payload.url : task.pr,
      updatedAt: event.at,
    }

    this.host.sync.store(updated, [{ event, accepted: true }])
    this.host.bus.publish({ kind: 'task.updated', task: updated, event, accepted: true })
    return updated
  }

  // Writes down a review comment that was sent to the agent of the task.
  recordReviewComment(taskId: string, text: string): Task {
    const task = this.host.getTask(taskId)
    const event: TaskEvent = {
      taskId,
      source: 'human',
      type: 'review.commented',
      at: this.host.timestamp(),
      payload: { text },
    }
    const updated: Task = { ...task, updatedAt: event.at }

    this.host.sync.store(updated, [{ event, accepted: true }])
    this.host.bus.publish({ kind: 'task.updated', task: updated, event, accepted: true })
    return updated
  }

  // Writes down that an agent was started in the task's terminal, and names it on the task.
  // The program is the command the shell looks for, to tell when it was not found.
  // The agent of a stage that starts its own (`stage`) is not the task's agent. Neither is one
  // restored in its shell after a restart (`restoredIn`, the shell).
  recordAgentLaunch(
    taskId: string,
    agent: string,
    program?: string,
    stage?: string,
    restoredIn?: string,
  ): Task {
    const task = this.host.getTask(taskId)
    const event: TaskEvent = {
      taskId,
      source: 'human',
      type: 'agent.launched',
      at: this.host.timestamp(),
      payload: {
        agent,
        ...(program === undefined ? {} : { program }),
        ...(stage === undefined ? {} : { stage }),
        ...(restoredIn === undefined ? {} : { terminal: restoredIn, restored: true }),
      },
    }
    const updated: Task = {
      ...task,
      agent: stage === undefined && restoredIn === undefined ? agent : task.agent,
      updatedAt: event.at,
    }

    this.host.sync.store(updated, [{ event, accepted: true }])
    this.host.bus.publish({ kind: 'task.updated', task: updated, event, accepted: true })
    return updated
  }

  // Keeps what GitHub says about the task's pull request. Logs only what changed: the pull request
  // was merged (which finishes a task in review) or closed, or its checks passed or failed.
  recordPullRequestStatus(taskId: string, status: PullRequestStatus): Task {
    const task = this.host.getTask(taskId)
    const before = task.prStatus
    if (before?.state === status.state && before.checks === status.checks) {
      return task
    }

    const at = this.host.timestamp()
    const types = pullRequestEventTypes(before, status)
    const { moving, logged } = applyPullRequestEvents(task, types, this.host.stepsOf(task), at)
    const updated: Task = { ...moving, prStatus: status, updatedAt: at }

    this.host.sync.store(updated, logged)
    this.publishPullRequestStatus(task, updated, logged)
    return updated
  }

  private publishPullRequestStatus(task: Task, updated: Task, logged: Logged): void {
    for (const { event, accepted } of logged) {
      this.host.bus.publish({
        kind: 'task.updated',
        task: updated,
        event,
        accepted,
        moved: updated.stage !== task.stage,
      })
    }
    if (logged.length === 0) {
      this.host.bus.publish({ kind: 'task.changed', task: updated })
    }
  }

  // Links the task to the item that tracks it elsewhere: a GitHub issue, a Jira ticket.
  linkIssue(taskId: string, rawLink: unknown, source: TaskEvent['source'] = 'human'): Task {
    const task = this.host.getTask(taskId)
    const issue: IssueLink = issueLinkSchema.parse(rawLink)
    const event: TaskEvent = {
      taskId,
      source,
      type: 'issue.linked',
      at: this.host.timestamp(),
      payload: { ...issue },
    }
    const updated: Task = { ...task, issue, updatedAt: event.at }

    this.host.sync.store(updated, [{ event, accepted: true }])
    this.host.bus.publish({ kind: 'task.updated', task: updated, event, accepted: true })
    return updated
  }

  // Chooses whether a stage of the task runs as a workflow. The stage runner reads the choice the
  // next time the stage starts, so a running stage keeps its prompt. Turning it off always works.
  setWorkflow(taskId: string, stageId: string, on: boolean): Task {
    const task = this.host.getTask(taskId)
    const stages = this.host.stepsOf(task).flatMap(stagesOf)
    const stage = stages.find((each) => each.id === stageId)
    if (stage === undefined) {
      throw new TaskEditError(`${taskId} has no stage "${stageId}"`)
    }
    if (on && !offersWorkflow(stage)) {
      throw new TaskEditError(
        `The ${stageName(stage)} stage cannot run with a workflow. Turn it on for the stage in the project settings first`,
      )
    }
    if (on && !canRunWorkflow(task.agent)) {
      throw new TaskEditError(`Only Claude Code can run a workflow; ${taskId} uses ${task.agent}`)
    }
    const workflowStages = on
      ? [...new Set([...task.workflowStages, stageId])]
      : task.workflowStages.filter((id) => id !== stageId)
    if (workflowStages.length === task.workflowStages.length) {
      return task
    }

    const event: TaskEvent = {
      taskId,
      source: 'human',
      type: 'stage.workflow-set',
      at: this.host.timestamp(),
      payload: { stage: stageId, on },
    }
    const updated: Task = { ...task, workflowStages, updatedAt: event.at }

    this.host.sync.store(updated, [{ event, accepted: true }])
    this.host.bus.publish({ kind: 'task.updated', task: updated, event, accepted: true })
    return updated
  }

  // Whether the stage waits for CI before it passes. The project settings must offer it.
  setCi(taskId: string, stageId: string, on: boolean): Task {
    const task = this.host.getTask(taskId)
    const stages = this.host.stepsOf(task).flatMap(stagesOf)
    const stage = stages.find((each) => each.id === stageId)
    if (stage === undefined) {
      throw new TaskEditError(`${taskId} has no stage "${stageId}"`)
    }
    if (on && !offersCi(stage)) {
      throw new TaskEditError(
        `The ${stageName(stage)} stage cannot be checked on CI. Turn it on for the stage in the project settings first`,
      )
    }
    const before = task.ciStages ?? []
    const ciStages = on ? [...new Set([...before, stageId])] : before.filter((id) => id !== stageId)
    if (ciStages.length === before.length) {
      return task
    }

    const event: TaskEvent = {
      taskId,
      source: 'human',
      type: 'stage.ci-set',
      at: this.host.timestamp(),
      payload: { stage: stageId, on },
    }
    const updated: Task = { ...task, ciStages, updatedAt: event.at }

    this.host.sync.store(updated, [{ event, accepted: true }])
    this.host.bus.publish({ kind: 'task.updated', task: updated, event, accepted: true })
    return updated
  }

  // Marks a later stage to skip: the stage runner skips it each time the task reaches it. The
  // current stage has its own Skip. A mark comes off at any stage, but not once the task is over.
  setSkip(taskId: string, stageId: string, on: boolean): Task {
    const task = this.host.getTask(taskId)
    const steps = this.host.stepsOf(task)
    const stage = steps.flatMap(stagesOf).find((each) => each.id === stageId)
    if (stage === undefined) {
      throw new TaskEditError(`${taskId} has no stage "${stageId}"`)
    }
    if (task.state === 'done' || task.state === 'cancelled') {
      throw new TaskEditError(`${taskId} is ${task.state}, so it has no stage left to skip`)
    }
    if (on && stage.kind === 'context') {
      throw new TaskEditError('The context stage comes first, so it cannot be skipped early')
    }
    const target = stepIndex(steps, stageId)
    const at = stepIndex(steps, task.stage)
    // Stages of one path share the index of their branch.
    const earlier = (pathOf(steps, stageId)?.index ?? 0) <= (pathOf(steps, task.stage)?.index ?? 0)
    if (on && (target < at || (target === at && earlier))) {
      throw new TaskEditError(
        `${taskId} is at or past the ${stageName(stage)} stage. Use Skip on the current stage`,
      )
    }
    const before = task.skipStages ?? []
    const skipStages = on
      ? [...new Set([...before, stageId])]
      : before.filter((id) => id !== stageId)
    if (skipStages.length === before.length) {
      return task
    }

    const event: TaskEvent = {
      taskId,
      source: 'human',
      type: 'stage.skip-set',
      at: this.host.timestamp(),
      payload: { stage: stageId, on },
    }
    const updated: Task = { ...task, skipStages, updatedAt: event.at }

    this.host.sync.store(updated, [{ event, accepted: true }])
    this.host.bus.publish({ kind: 'task.updated', task: updated, event, accepted: true })
    return updated
  }

  // Marks a task as left to a person, or not. Only the stage runner reads it, so no event.
  setWithoutAgent(taskId: string, on: boolean): void {
    const task = this.host.getTask(taskId)
    if ((task.withoutAgent === true) !== on) {
      // Left out, not undefined: the YAML of the task file cannot hold undefined.
      const { withoutAgent: _withoutAgent, ...rest } = task
      this.host.sync.store(on ? { ...task, withoutAgent: true } : rest, [])
    }
  }

  // Writes down an event that changes no state, such as fetched context.
  recordEvent(
    taskId: string,
    type: EventType,
    payload: Record<string, unknown>,
    source: TaskEvent['source'] = 'system',
  ): Task {
    const task = this.host.getTask(taskId)
    const event: TaskEvent = { taskId, source, type, at: this.host.timestamp(), payload }
    const updated: Task = { ...task, updatedAt: event.at }

    this.host.sync.store(updated, [{ event, accepted: true }])
    this.host.bus.publish({ kind: 'task.updated', task: updated, event, accepted: true })
    return updated
  }

  // Moves the time of the last update of a running task to its agent's last sign of life. No line
  // in the log: dashboards read the log again for each logged event, so a busy agent would make
  // them read it every 30 seconds. At most once per `minIntervalMs`, except that the first sign of
  // life ends a wait for background work at once. Terminal output alone is no such sign: a redraw
  // after a resize prints too while the agent still waits. So with `endsWait: false` the wait
  // stays. Returns whether it changed anything.
  recordActivity(
    taskId: string,
    { minIntervalMs = ACTIVITY_INTERVAL_MS, endsWait = true }: ActivityOptions = {},
  ): boolean {
    const task = this.host.getTask(taskId)
    if (task.state !== 'running') {
      return false
    }

    const at = this.host.timestamp()
    const endsWaitNow = endsWait && task.backgroundSince !== null
    if (!endsWaitNow && Date.parse(at) - Date.parse(task.updatedAt) < minIntervalMs) {
      return false
    }

    const updated: Task = {
      ...task,
      backgroundSince: endsWait ? null : task.backgroundSince,
      updatedAt: at,
    }
    // When only the time moves, the file can wait: the next real change writes it anyway.
    const writtenAt = this.host.sync.writtenAt.get(taskId)
    const writeFile =
      updated.backgroundSince !== task.backgroundSince ||
      writtenAt === undefined ||
      Date.parse(at) - Date.parse(writtenAt) >= ACTIVITY_FILE_INTERVAL_MS

    this.host.sync.store(updated, [], writeFile)
    this.host.bus.publish({ kind: 'task.changed', task: updated })
    return true
  }
}
