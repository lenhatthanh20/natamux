import { z } from 'zod'
import { TASK_PRIORITIES } from './task.js'

export const templateIdSchema = z
  .string()
  .regex(/^[a-z0-9]+(-[a-z0-9]+)*$/, 'A template id is lower case words joined by "-"')
  .max(60)

// A task to start from: what a new task copies. Kept as a markdown file, like a task.
export const taskTemplateSchema = z.object({
  id: templateIdSchema,
  name: z.string().trim().min(1).max(100),
  title: z.string().trim().min(1).max(200),
  description: z.string().default(''),
  priority: z.enum(TASK_PRIORITIES).default('normal'),
  labels: z.array(z.string().trim().min(1)).default([]),
})

export type TaskTemplate = z.infer<typeof taskTemplateSchema>
