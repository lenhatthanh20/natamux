import {
  applyTaskEvent,
  type EventType,
  makesCycle,
  type PullRequestStatus,
  RECORD_ONLY_EVENT_TYPES,
  TASK_PRIORITIES,
  type Task,
  type TaskEvent,
  type TaskState,
  taskEventSchema,
  taskIdSchema,
  taskSchema,
} from '@helmsman/core'
import { z } from 'zod'
import type { EventBus } from './event-bus.js'
import { listTaskFiles, listTaskIds, readTaskFile, writeTaskFile } from './task-files.js'
import type { StoredEvent, TaskIndex } from './task-index.js'

export const createTaskInputSchema = taskSchema.pick({
  title: true,
  description: true,
  priority: true,
  labels: true,
  agent: true,
  dependsOn: true,
  parentId: true,
})

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
}

export class TaskService {
  private readonly dataDir: string
  private readonly index: TaskIndex
  private readonly bus: EventBus
  private readonly now: () => Date

  constructor(options: TaskServiceOptions) {
    this.dataDir = options.dataDir
    this.index = options.index
    this.bus = options.bus
    this.now = options.now ?? (() => new Date())
  }

  // Rebuild the index cache from the task files.
  init(): void {
    this.index.replaceAllTasks(listTaskFiles(this.dataDir))
  }

  createTask(rawInput: unknown): Task {
    const input = createTaskInputSchema.parse(rawInput)
    if (input.parentId) {
      this.checkParent(input.parentId)
    }
    const at = this.timestamp()
    const task = taskSchema.parse({
      ...input,
      id: this.nextTaskId(),
      state: 'draft',
      createdAt: at,
      updatedAt: at,
    })

    writeTaskFile(this.dataDir, task)
    this.index.upsertTask(task)
    this.index.appendEvent(
      { taskId: task.id, source: 'system', type: 'task.created', at, payload: {} },
      true,
    )
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
    this.index.appendEvent(event, true)
    this.bus.publish({ kind: 'task.updated', task: updated, event, accepted: true })
    return updated
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
    const event: TaskEvent = taskEventSchema.parse({ ...input, taskId, at: this.timestamp() })

    const { state, accepted } = applyTaskEvent(task.state, event)
    const updated: Task = accepted ? { ...task, state, updatedAt: event.at } : task

    if (accepted) {
      writeTaskFile(this.dataDir, updated)
      this.index.upsertTask(updated)
    }
    this.index.appendEvent(event, accepted)
    this.bus.publish({ kind: 'task.updated', task: updated, event, accepted })
    return { task: updated, accepted }
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
    this.index.appendEvent(event, true)
    this.bus.publish({ kind: 'task.updated', task: updated, event, accepted: true })
    return updated
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
    this.index.appendEvent(event, true)
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
    this.index.appendEvent(event, true)
    this.bus.publish({ kind: 'task.updated', task: updated, event, accepted: true })
    return updated
  }

  // Writes down that an agent was started in the task's terminal, and names it on the task.
  recordAgentLaunch(taskId: string, agent: string): Task {
    const task = this.getTask(taskId)
    const event: TaskEvent = {
      taskId,
      source: 'human',
      type: 'agent.launched',
      at: this.timestamp(),
      payload: { agent },
    }
    const updated: Task = { ...task, agent, updatedAt: event.at }

    writeTaskFile(this.dataDir, updated)
    this.index.upsertTask(updated)
    this.index.appendEvent(event, true)
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

    let state = task.state
    const logged: { event: TaskEvent; accepted: boolean }[] = []
    for (const type of types) {
      const event: TaskEvent = { taskId, source: 'git', type, at, payload: { url: task.pr } }
      const applied = applyTaskEvent(state, event)
      state = applied.state
      logged.push({ event, accepted: applied.accepted })
    }
    const updated: Task = { ...task, state, prStatus: status, updatedAt: at }

    writeTaskFile(this.dataDir, updated)
    this.index.upsertTask(updated)
    for (const { event, accepted } of logged) {
      this.index.appendEvent(event, accepted)
      this.bus.publish({ kind: 'task.updated', task: updated, event, accepted })
    }
    if (logged.length === 0) {
      this.bus.publish({ kind: 'task.changed', task: updated })
    }
    return updated
  }

  // Writes down that the agent of a running task is still at work, so the time of its last update
  // is the time of its last sign of life. At most once per `minIntervalMs`, so a busy agent does
  // not fill the log. Returns whether it wrote anything.
  recordActivity(taskId: string, minIntervalMs = ACTIVITY_INTERVAL_MS): boolean {
    const task = this.getTask(taskId)
    if (task.state !== 'running') {
      return false
    }

    const at = this.timestamp()
    if (Date.parse(at) - Date.parse(task.updatedAt) < minIntervalMs) {
      return false
    }

    const event: TaskEvent = { taskId, source: 'agent', type: 'agent.active', at, payload: {} }
    const updated: Task = { ...task, updatedAt: at }

    writeTaskFile(this.dataDir, updated)
    this.index.upsertTask(updated)
    this.index.appendEvent(event, true)
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
