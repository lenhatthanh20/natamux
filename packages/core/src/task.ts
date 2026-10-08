import { z } from 'zod'
import { taskStateSchema } from './task-state.js'

export const TASK_PRIORITIES = ['low', 'normal', 'high', 'urgent'] as const

export const PULL_REQUEST_STATES = ['open', 'draft', 'merged', 'closed'] as const
export const CHECK_STATES = ['passing', 'failing', 'pending', 'none'] as const

// What GitHub last said about the task's pull request and its checks (CI).
export const pullRequestStatusSchema = z.object({
  state: z.enum(PULL_REQUEST_STATES),
  checks: z.enum(CHECK_STATES),
})

export type PullRequestStatus = z.infer<typeof pullRequestStatusSchema>

export const taskIdSchema = z.string().regex(/^T-\d+$/, 'Task id looks like T-42')

export const taskSchema = z.object({
  id: taskIdSchema,
  title: z.string().trim().min(1).max(200),
  description: z.string().default(''),
  state: taskStateSchema.default('draft'),
  priority: z.enum(TASK_PRIORITIES).default('normal'),
  labels: z.array(z.string().min(1)).default([]),
  agent: z.string().min(1).nullable().default(null),
  branch: z.string().min(1).nullable().default(null),
  workspace: z.string().min(1).nullable().default(null),
  pr: z.url().nullable().default(null),
  prStatus: pullRequestStatusSchema.nullable().default(null),
  dependsOn: z.array(taskIdSchema).default([]),
  // The task this one is a part of. One level only: a subtask has no subtasks.
  parentId: taskIdSchema.nullable().default(null),
  createdAt: z.iso.datetime(),
  updatedAt: z.iso.datetime(),
})

export type Task = z.infer<typeof taskSchema>
