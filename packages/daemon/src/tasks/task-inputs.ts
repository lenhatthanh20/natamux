import {
  RECORD_ONLY_EVENT_TYPES,
  TASK_PRIORITIES,
  type Task,
  taskContextSchema,
  taskEventSchema,
  taskIdSchema,
  taskSchema,
} from '@natamux/core'
import { z } from 'zod'

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

export const EDITABLE_FIELDS = ['title', 'description', 'priority', 'labels', 'dependsOn'] as const

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

export class TaskEditError extends Error {
  constructor(message: string) {
    super(message)
    this.name = 'TaskEditError'
  }
}

// The task file changed since this service read or wrote it, so a write would lose that edit.
export class TaskFileChangedError extends Error {
  constructor(taskId: string) {
    super(`The file of task ${taskId} was changed outside Natamux. It is read again; try again`)
    this.name = 'TaskFileChangedError'
  }
}
