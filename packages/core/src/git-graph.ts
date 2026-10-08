import { z } from 'zod'

// One commit in the history of a task branch.
export const graphCommitSchema = z.object({
  hash: z.string().min(1),
  parents: z.array(z.string().min(1)),
  author: z.string(),
  date: z.string(),
  subject: z.string(),
  // Branch and tag names that point at this commit, as `git log --decorate` shows them.
  refs: z.array(z.string()),
  // True when the commit is on the task branch and not on the base branch yet.
  onBranch: z.boolean(),
})

export type GraphCommit = z.infer<typeof graphCommitSchema>

// The history of a task branch and of the branch it came from, newest first. `base` is the branch
// that the main checkout is on; it is null when that checkout is not on a branch, and then
// `ahead` and `behind` are null too.
export const gitGraphSchema = z.object({
  branch: z.string().nullable(),
  head: z.string().min(1),
  base: z.string().nullable(),
  ahead: z.number().int().nonnegative().nullable(),
  behind: z.number().int().nonnegative().nullable(),
  commits: z.array(graphCommitSchema),
  // True when there are more commits than the list holds.
  truncated: z.boolean(),
})

export type GitGraph = z.infer<typeof gitGraphSchema>
