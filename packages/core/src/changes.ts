import { z } from 'zod'

// M modified, A added, D deleted, R renamed, U not tracked yet, C conflict.
export const CHANGE_STATUSES = ['M', 'A', 'D', 'R', 'U', 'C'] as const

export const changeStatusSchema = z.enum(CHANGE_STATUSES)
export type ChangeStatus = z.infer<typeof changeStatusSchema>

// One file that differs from the last commit of a workspace.
export const changeEntrySchema = z.object({
  path: z.string().min(1),
  status: changeStatusSchema,
  originalPath: z.string().nullable(),
})

export type ChangeEntry = z.infer<typeof changeEntrySchema>

// Both sides of one changed file. A side is null when the file did not exist there, or when it is
// binary or too large to send (then `binary` or `tooLarge` says so).
export const fileDiffSchema = z.object({
  path: z.string().min(1),
  status: changeStatusSchema,
  original: z.string().nullable(),
  modified: z.string().nullable(),
  binary: z.boolean(),
  tooLarge: z.boolean(),
})

export type FileDiff = z.infer<typeof fileDiffSchema>
