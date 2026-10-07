import {
  applyTaskEvent,
  type Task,
  type TaskEvent,
  type TaskState,
  taskEventSchema,
  taskSchema,
} from '@helmsman/core'
import type { z } from 'zod'
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
})

export const applyEventInputSchema = taskEventSchema
  .pick({ source: true, type: true, payload: true })
  .refine((input) => input.type !== 'task.created', {
    message: 'task.created is recorded by the daemon, not sent by clients',
    path: ['type'],
  })

export type CreateTaskInput = z.input<typeof createTaskInputSchema>
export type ApplyEventInput = z.input<typeof applyEventInputSchema>

export type ApplyEventResult = {
  task: Task
  accepted: boolean
}

export class TaskNotFoundError extends Error {
  readonly taskId: string

  constructor(taskId: string) {
    super(`Task ${taskId} does not exist`)
    this.name = 'TaskNotFoundError'
    this.taskId = taskId
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
