import { z } from 'zod'
import { taskStateSchema } from './task-state.js'

export const TASK_PRIORITIES = ['low', 'normal', 'high', 'urgent'] as const

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
  dependsOn: z.array(taskIdSchema).default([]),
  createdAt: z.iso.datetime(),
  updatedAt: z.iso.datetime(),
})

export type Task = z.infer<typeof taskSchema>
