import { readdirSync, rmSync } from 'node:fs'
import { join } from 'node:path'
import {
  activeSteps,
  applyTaskEvent,
  canRunWorkflow,
  completeEvent,
  DEFAULT_PROJECT_SETTINGS,
  type EventType,
  firstPosition,
  type IssueLink,
  issueLinkSchema,
  LEGACY_PIPELINE,
  makesCycle,
  offersWorkflow,
  type ProjectSettings,
  type PullRequestStatus,
  RECORD_ONLY_EVENT_TYPES,
  resolveSteps,
  type StagePosition,
  type Step,
  stageName,
  stagesOf,
  stepRefsOf,
  TASK_PRIORITIES,
  type Task,
  type TaskEvent,
  type TaskState,
  taskContextSchema,
  taskEventSchema,
  taskIdSchema,
  taskSchema,
} from '@natamux/core'
import { z } from 'zod'
import { type AttachmentFile, saveAttachments } from './attachments.js'
import type { EventBus } from './event-bus.js'
import { appendEventLine, eventLogPath, readEventLog, writeEventLog } from './event-log.js'
import { archiveDir } from './stage-files.js'
import {
  listTaskFiles,
  listTaskIds,
  readTaskFile,
  taskFilePath,
  writeTaskFile,
} from './task-files.js'
import type { StoredEvent, TaskIndex } from './task-index.js'

export const createTaskInputSchema = taskSchema
  .pick({
    title: true,
    description: true,
    priority: true,
    labels: true,
    agent: true,
    dependsOn: true,
    parentId: true,
    repo: true,
  })
  .extend({ context: taskContextSchema.nullable().optional() })

// The fields a person may change after a task is created. State has its own events.
// Built without the defaults of taskSchema, so a field that is left out stays as it is.
export const updateTaskInputSchema = z
  .object({
    title: taskSchema.shape.title,
    description: z.string(),
    priority: z.enum(TASK_PRIORITIES),
    labels: z.array(z.string().trim().min(1)),
    dependsOn: z.array(taskIdSchema),
  })
  .partial()
  .strict()

const EDITABLE_FIELDS = ['title', 'description', 'priority', 'labels', 'dependsOn'] as const

// How often a running task's sign of life is written down at most.
export const ACTIVITY_INTERVAL_MS = 30_000

export type ActivityOptions = { minIntervalMs?: number; endsWait?: boolean }

export const applyEventInputSchema = taskEventSchema
  .pick({ source: true, type: true, payload: true })
  .refine((input) => !RECORD_ONLY_EVENT_TYPES.includes(input.type), {
    message: 'This event is recorded by the daemon, not sent by clients',
    path: ['type'],
  })

export type CreateTaskInput = z.input<typeof createTaskInputSchema>
export type ApplyEventInput = z.input<typeof applyEventInputSchema>

export type ApplyEventResult = {
  task: Task
  accepted: boolean
  moved: boolean
}

export type WorkspaceChange = {
  workspace: string | null
  branch: string | null
}

export class TaskNotFoundError extends Error {
  readonly taskId: string

  constructor(taskId: string) {
    super(`Task ${taskId} does not exist`)
    this.name = 'TaskNotFoundError'
    this.taskId = taskId
  }
}

export type SettingsOf = (repo: string | null) => ProjectSettings

// A done or cancelled task needs its shells no more. Neither does a task that is gone.
export function isTaskEnded(
  tasks: { getTask(taskId: string): Pick<Task, 'state'> },
  taskId: string,
): boolean {
  try {
    const { state } = tasks.getTask(taskId)
    return state === 'done' || state === 'cancelled'
  } catch {
    return true
  }
}

// The flow tasks had before stages: implement, review, release. The daemon passes the settings of
// each repository instead.
const LEGACY_SETTINGS: ProjectSettings = { ...DEFAULT_PROJECT_SETTINGS, stages: LEGACY_PIPELINE }

function positionOf(task: Pick<Task, 'stage' | 'state' | 'lanes'>): StagePosition {
  return { stage: task.stage, state: task.state, lanes: task.lanes }
}

// The task at its new position. Entering a new step starts its clock.
function moveTask(task: Task, position: StagePosition, moved: boolean, at: string): Task {
  return {
    ...task,
    stage: position.stage,
    state: position.state,
    lanes: position.lanes,
    stageSince: moved ? at : task.stageSince,
  }
}

// An agent waits for its background work from its first `agent.waiting` until it shows up again,
// or the task leaves running. Other events, such as checks that pass, do not end the wait.
function backgroundSinceAfter(task: Task, event: TaskEvent, state: TaskState): string | null {
  if (event.type === 'agent.waiting') {
    return task.backgroundSince ?? event.at
  }
  return state !== 'running' || event.source === 'agent' ? null : task.backgroundSince
}

export class TaskEditError extends Error {
  constructor(message: string) {
    super(message)
    this.name = 'TaskEditError'
  }
}

type TaskServiceOptions = {
  dataDir: string
  index: TaskIndex
  bus: EventBus
  now?: () => Date
  // The settings of a repository (`.natamux.json`). Default: the legacy pipeline.
  settingsOf?: SettingsOf
}

export class TaskService {
  private readonly dataDir: string
  private readonly index: TaskIndex
  private readonly bus: EventBus
  private readonly now: () => Date
  readonly settingsOf: SettingsOf

  constructor(options: TaskServiceOptions) {
    this.dataDir = options.dataDir
    this.index = options.index
    this.bus = options.bus
    this.now = options.now ?? (() => new Date())
    this.settingsOf = options.settingsOf ?? (() => LEGACY_SETTINGS)
  }

  // Rebuild the index cache from the task files and their event logs. A task with no event log
  // yet (made before there were logs) gets one from the index.
  init(): void {
    const tasks = listTaskFiles(this.dataDir)
    this.index.replaceAllTasks(tasks)
    for (const task of tasks) {
      const logged = readEventLog(this.dataDir, task.id)
      if (logged === undefined) {
        writeEventLog(this.dataDir, task.id, this.index.listEvents(task.id))
      } else if (logged.length !== this.index.countEvents(task.id)) {
        this.index.replaceEvents(task.id, logged)
      }
    }
  }

  // The steps of a task, with each stage's settings from its repository.
  stepsOf(task: Pick<Task, 'stages' | 'repo'>): Step[] {
    return resolveSteps(task.stages, this.settingsOf(task.repo).stages)
  }

  // The event log file first, then the index: a crash between the two loses nothing.
  private logEvent(event: TaskEvent, accepted: boolean): void {
    appendEventLine(this.dataDir, event, accepted)
    this.index.appendEvent(event, accepted)
  }

  // The attachments are written before the task is published, so its context stage, which starts
  // at once, finds them.
  createTask(rawInput: unknown, attachments: readonly AttachmentFile[] = []): Task {
    const input = createTaskInputSchema.parse(rawInput)
    if (input.parentId) {
      this.checkParent(input.parentId)
    }
    const at = this.timestamp()
    // Stages that are off stay in the settings, but a new task does not get them.
    const steps = activeSteps(this.settingsOf(input.repo ?? null).stages)
    const start = firstPosition(steps)
    const task = taskSchema.parse({
      ...input,
      id: this.nextTaskId(),
      stage: start.stage,
      state: start.state,
      lanes: start.lanes,
      stages: stepRefsOf(steps),
      stageSince: at,
      createdAt: at,
      updatedAt: at,
    })

    writeTaskFile(this.dataDir, task)
    this.index.upsertTask(task)
    this.logEvent(
      { taskId: task.id, source: 'system', type: 'task.created', at, payload: {} },
      true,
    )
    saveAttachments(this.dataDir, task.id, attachments)
    for (const { name, bytes } of attachments) {
      this.logEvent(
        {
          taskId: task.id,
          source: 'system',
          type: 'attachment.added',
          at,
          payload: { name, size: bytes.length },
        },
        true,
      )
    }
    this.bus.publish({ kind: 'task.created', task })
    return task
  }

  getTask(taskId: string): Task {
    const task = readTaskFile(this.dataDir, taskId)
    if (!task) {
      throw new TaskNotFoundError(taskId)
    }
    return task
  }

  listTasks(state?: TaskState): Task[] {
    return this.index.listTasks(state)
  }

  listEvents(taskId: string): StoredEvent[] {
    this.getTask(taskId)
    return this.index.listEvents(taskId)
  }

  // Changes the details of a task. Logs `task.edited` with the names of the fields that changed,
  // and writes nothing when none did.
  updateTask(taskId: string, rawInput: unknown): Task {
    const task = this.getTask(taskId)
    const input = updateTaskInputSchema.parse(rawInput)
    if (input.dependsOn !== undefined) {
      this.checkDependencies(taskId, input.dependsOn)
    }
    const next = { ...task, ...input }
    const changed = EDITABLE_FIELDS.filter(
      (field) => JSON.stringify(next[field]) !== JSON.stringify(task[field]),
    )
    if (changed.length === 0) {
      return task
    }

    const event: TaskEvent = {
      taskId,
      source: 'human',
      type: 'task.edited',
      at: this.timestamp(),
      payload: { fields: changed },
    }
    const updated: Task = taskSchema.parse({ ...next, updatedAt: event.at })

    writeTaskFile(this.dataDir, updated)
    this.index.upsertTask(updated)
    this.logEvent(event, true)
    this.bus.publish({ kind: 'task.updated', task: updated, event, accepted: true })
    return updated
  }

  // Removes the task and everything stored with it. A workspace is refused here: the caller
  // removes that first, so a worktree is never left behind with no task.
  deleteTask(taskId: string): Task[] {
    const task = this.getTask(taskId)
    if (task.workspace !== null) {
      throw new TaskEditError(`${taskId} still has a workspace`)
    }
    const related = this.detach(taskId)
    this.removeStoredTask(taskId)
    this.bus.publish({ kind: 'task.deleted', taskId })
    return related
  }

  // A deleted task must not stay as a parent or a dependency of the tasks that remain.
  private detach(taskId: string): Task[] {
    const changed: Task[] = []
    for (const other of this.listTasks()) {
      if (other.id === taskId) {
        continue
      }
      const fields: string[] = []
      let parentId = other.parentId
      let dependsOn = other.dependsOn
      if (other.parentId === taskId) {
        parentId = null
        fields.push('parentId')
      }
      if (other.dependsOn.includes(taskId)) {
        dependsOn = other.dependsOn.filter((id) => id !== taskId)
        fields.push('dependsOn')
      }
      if (fields.length === 0) {
        continue
      }
      const event: TaskEvent = {
        taskId: other.id,
        source: 'system',
        type: 'task.edited',
        at: this.timestamp(),
        payload: { fields },
      }
      const updated = taskSchema.parse({ ...other, parentId, dependsOn, updatedAt: event.at })
      writeTaskFile(this.dataDir, updated)
      this.index.upsertTask(updated)
      this.logEvent(event, true)
      this.bus.publish({ kind: 'task.updated', task: updated, event, accepted: true })
      changed.push(updated)
    }
    return changed
  }

  private removeStoredTask(taskId: string): void {
    rmSync(taskFilePath(this.dataDir, taskId), { force: true })
    rmSync(eventLogPath(this.dataDir, taskId), { force: true })
    rmSync(archiveDir(this.dataDir, taskId), { recursive: true, force: true })
    this.removeLaunchScripts(taskId)
    this.index.deleteTask(taskId)
  }

  // Launch scripts are named agent-launch-T-0001.sh and agent-launch-T-0001-2.sh.
  private removeLaunchScripts(taskId: string): void {
    let names: string[]
    try {
      names = readdirSync(this.dataDir)
    } catch (error) {
      if (error instanceof Error && 'code' in error && error.code === 'ENOENT') {
        return
      }
      throw error
    }
    const exact = `agent-launch-${taskId}.sh`
    const prefix = `agent-launch-${taskId}-`
    for (const name of names) {
      if (name === exact || (name.startsWith(prefix) && name.endsWith('.sh'))) {
        rmSync(join(this.dataDir, name), { force: true })
      }
    }
  }

  // Subtasks are one level deep: the parent must exist and must not be a subtask itself.
  private checkParent(parentId: string): void {
    const parent = readTaskFile(this.dataDir, parentId)
    if (!parent) {
      throw new TaskEditError(`No such task: ${parentId}`)
    }
    if (parent.parentId !== null) {
      throw new TaskEditError(`${parentId} is a subtask; a subtask cannot have subtasks`)
    }
  }

  private checkDependencies(taskId: string, dependsOn: readonly string[]): void {
    const tasks = this.listTasks()
    const known = new Set(tasks.map((task) => task.id))
    const unknown = dependsOn.filter((id) => !known.has(id))
    if (unknown.length > 0) {
      throw new TaskEditError(`No such task: ${unknown.join(', ')}`)
    }
    if (makesCycle(taskId, dependsOn, tasks)) {
      throw new TaskEditError('These dependencies would make a loop. A task cannot wait for itself')
    }
  }

  applyEvent(taskId: string, rawInput: unknown): ApplyEventResult {
    const task = this.getTask(taskId)
    const input = applyEventInputSchema.parse(rawInput)
    const steps = this.stepsOf(task)
    // A button or `natamux ask` may leave out the stage; the log keeps the one it was about.
    const event: TaskEvent = completeEvent(
      positionOf(task),
      taskEventSchema.parse({ ...input, taskId, at: this.timestamp() }),
      steps,
    )

    const { position, accepted, moved } = applyTaskEvent(positionOf(task), event, steps)
    const updated: Task = accepted
      ? {
          ...moveTask(task, position, moved, event.at),
          backgroundSince: backgroundSinceAfter(task, event, position.state),
          updatedAt: event.at,
        }
      : task

    if (accepted) {
      writeTaskFile(this.dataDir, updated)
      this.index.upsertTask(updated)
    }
    this.logEvent(event, accepted)
    this.bus.publish({ kind: 'task.updated', task: updated, event, accepted, moved })
    return { task: updated, accepted, moved }
  }

  // Records where the task's working copy lives. Called by the workspace service.
  recordWorkspace(taskId: string, change: WorkspaceChange): Task {
    const task = this.getTask(taskId)
    const event: TaskEvent = {
      taskId,
      source: 'system',
      type: change.workspace === null ? 'workspace.removed' : 'workspace.created',
      at: this.timestamp(),
      payload: { workspace: change.workspace ?? task.workspace, branch: change.branch },
    }
    const updated: Task = {
      ...task,
      workspace: change.workspace,
      branch: change.branch,
      updatedAt: event.at,
    }

    writeTaskFile(this.dataDir, updated)
    this.index.upsertTask(updated)
    this.logEvent(event, true)
    this.bus.publish({ kind: 'task.updated', task: updated, event, accepted: true })
    return updated
  }

  // Natamux ended the agents of the task, for example with its workspace. A stage whose agent ran
  // or waited for you would stay so with no agent, and would show up in Needs you later.
  stopAgents(taskId: string): Task {
    const task = this.getTask(taskId)
    const lanes: [string, TaskState][] =
      Object.keys(task.lanes).length > 0 ? Object.entries(task.lanes) : [[task.stage, task.state]]
    for (const [stage, state] of lanes) {
      if (state === 'running' || state === 'needs-input') {
        this.applyEvent(taskId, { source: 'system', type: 'agent.stopped', payload: { stage } })
      }
    }
    return this.getTask(taskId)
  }

  // Writes down a commit, a push or a new pull request. A pull request also sets `pr` of the task.
  recordGit(
    taskId: string,
    type: 'git.committed' | 'git.pushed' | 'pr.opened',
    payload: Record<string, string>,
  ): Task {
    const task = this.getTask(taskId)
    const event: TaskEvent = { taskId, source: 'git', type, at: this.timestamp(), payload }
    const updated: Task = {
      ...task,
      pr: type === 'pr.opened' && payload.url ? payload.url : task.pr,
      updatedAt: event.at,
    }

    writeTaskFile(this.dataDir, updated)
    this.index.upsertTask(updated)
    this.logEvent(event, true)
    this.bus.publish({ kind: 'task.updated', task: updated, event, accepted: true })
    return updated
  }

  // Writes down a review comment that was sent to the agent of the task.
  recordReviewComment(taskId: string, text: string): Task {
    const task = this.getTask(taskId)
    const event: TaskEvent = {
      taskId,
      source: 'human',
      type: 'review.commented',
      at: this.timestamp(),
      payload: { text },
    }
    const updated: Task = { ...task, updatedAt: event.at }

    writeTaskFile(this.dataDir, updated)
    this.index.upsertTask(updated)
    this.logEvent(event, true)
    this.bus.publish({ kind: 'task.updated', task: updated, event, accepted: true })
    return updated
  }

  // Writes down that an agent was started in the task's terminal, and names it on the task.
  // The program is the command the shell looks for, to tell when it was not found.
  recordAgentLaunch(taskId: string, agent: string, program?: string): Task {
    const task = this.getTask(taskId)
    const event: TaskEvent = {
      taskId,
      source: 'human',
      type: 'agent.launched',
      at: this.timestamp(),
      payload: program === undefined ? { agent } : { agent, program },
    }
    const updated: Task = { ...task, agent, updatedAt: event.at }

    writeTaskFile(this.dataDir, updated)
    this.index.upsertTask(updated)
    this.logEvent(event, true)
    this.bus.publish({ kind: 'task.updated', task: updated, event, accepted: true })
    return updated
  }

  // Keeps what GitHub says about the task's pull request. Logs only what changed: the pull request
  // was merged (which finishes a task in review) or closed, or its checks passed or failed.
  recordPullRequestStatus(taskId: string, status: PullRequestStatus): Task {
    const task = this.getTask(taskId)
    const before = task.prStatus
    if (before?.state === status.state && before.checks === status.checks) {
      return task
    }

    const at = this.timestamp()
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

    const steps = this.stepsOf(task)
    let moving = task
    const logged: { event: TaskEvent; accepted: boolean }[] = []
    for (const type of types) {
      const event: TaskEvent = { taskId, source: 'git', type, at, payload: { url: task.pr } }
      const applied = applyTaskEvent(positionOf(moving), event, steps)
      moving = moveTask(moving, applied.position, applied.moved, at)
      logged.push({ event, accepted: applied.accepted })
    }
    const updated: Task = { ...moving, prStatus: status, updatedAt: at }

    writeTaskFile(this.dataDir, updated)
    this.index.upsertTask(updated)
    for (const { event, accepted } of logged) {
      this.logEvent(event, accepted)
      this.bus.publish({
        kind: 'task.updated',
        task: updated,
        event,
        accepted,
        moved: updated.stage !== task.stage,
      })
    }
    if (logged.length === 0) {
      this.bus.publish({ kind: 'task.changed', task: updated })
    }
    return updated
  }

  // Links the task to the item that tracks it elsewhere: a GitHub issue, a Jira ticket.
  linkIssue(taskId: string, rawLink: unknown, source: TaskEvent['source'] = 'human'): Task {
    const task = this.getTask(taskId)
    const issue: IssueLink = issueLinkSchema.parse(rawLink)
    const event: TaskEvent = {
      taskId,
      source,
      type: 'issue.linked',
      at: this.timestamp(),
      payload: { ...issue },
    }
    const updated: Task = { ...task, issue, updatedAt: event.at }

    writeTaskFile(this.dataDir, updated)
    this.index.upsertTask(updated)
    this.logEvent(event, true)
    this.bus.publish({ kind: 'task.updated', task: updated, event, accepted: true })
    return updated
  }

  // Chooses whether a stage of the task runs as a workflow. The stage runner reads the choice the
  // next time the stage starts, so a running stage keeps its prompt. Turning it off always works.
  setWorkflow(taskId: string, stageId: string, on: boolean): Task {
    const task = this.getTask(taskId)
    const stages = this.stepsOf(task).flatMap(stagesOf)
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
      at: this.timestamp(),
      payload: { stage: stageId, on },
    }
    const updated: Task = { ...task, workflowStages, updatedAt: event.at }

    writeTaskFile(this.dataDir, updated)
    this.index.upsertTask(updated)
    this.logEvent(event, true)
    this.bus.publish({ kind: 'task.updated', task: updated, event, accepted: true })
    return updated
  }

  // Writes down an event that changes no state, such as fetched context.
  recordEvent(
    taskId: string,
    type: EventType,
    payload: Record<string, unknown>,
    source: TaskEvent['source'] = 'system',
  ): Task {
    const task = this.getTask(taskId)
    const event: TaskEvent = { taskId, source, type, at: this.timestamp(), payload }
    const updated: Task = { ...task, updatedAt: event.at }

    writeTaskFile(this.dataDir, updated)
    this.index.upsertTask(updated)
    this.logEvent(event, true)
    this.bus.publish({ kind: 'task.updated', task: updated, event, accepted: true })
    return updated
  }

  // Writes down that the agent of a running task is still at work, so the time of its last update
  // is the time of its last sign of life. At most once per `minIntervalMs`, so a busy agent does
  // not fill the log, except that the first sign of life ends a wait for background work at once.
  // Terminal output alone is no such sign: a redraw after a resize prints too while the agent
  // still waits. So with `endsWait: false` the wait stays. Returns whether it wrote anything.
  recordActivity(
    taskId: string,
    { minIntervalMs = ACTIVITY_INTERVAL_MS, endsWait = true }: ActivityOptions = {},
  ): boolean {
    const task = this.getTask(taskId)
    if (task.state !== 'running') {
      return false
    }

    const at = this.timestamp()
    const endsWaitNow = endsWait && task.backgroundSince !== null
    if (!endsWaitNow && Date.parse(at) - Date.parse(task.updatedAt) < minIntervalMs) {
      return false
    }

    const event: TaskEvent = { taskId, source: 'agent', type: 'agent.active', at, payload: {} }
    const updated: Task = {
      ...task,
      backgroundSince: endsWait ? null : task.backgroundSince,
      updatedAt: at,
    }

    writeTaskFile(this.dataDir, updated)
    this.index.upsertTask(updated)
    this.logEvent(event, true)
    this.bus.publish({ kind: 'task.updated', task: updated, event, accepted: true })
    return true
  }

  private timestamp(): string {
    return this.now().toISOString()
  }

  private nextTaskId(): string {
    const highest = listTaskIds(this.dataDir).reduce(
      (max, id) => Math.max(max, Number(id.slice(2))),
      0,
    )
    return `T-${String(highest + 1).padStart(4, '0')}`
  }
}
