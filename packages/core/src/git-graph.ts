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

const commitCount = z.number().int().nonnegative()

// Where the branch stands against `origin`, which is what a push changes.
// - none: the repository has no remote named origin.
// - new: origin has no branch of this name, because it was never pushed or was deleted there.
//   `unpushed` counts the commits that are on no branch of origin.
// - tracked: the counts compare HEAD with origin/<branch>, as git last saw it.
export const pushStatusSchema = z.discriminatedUnion('remote', [
  z.object({ remote: z.literal('none') }),
  z.object({ remote: z.literal('new'), unpushed: commitCount }),
  z.object({ remote: z.literal('tracked'), unpushed: commitCount, unpulled: commitCount }),
])

export type PushStatus = z.infer<typeof pushStatusSchema>

// The history of a task branch and of the branch it came from, newest first. `base` is the branch
// that the main checkout is on; it is null when that checkout is not on a branch, and then
// `ahead` and `behind` are null too. `push` is null when HEAD is not on a branch, and when a daemon
// from before it sends none.
export const gitGraphSchema = z.object({
  branch: z.string().nullable(),
  head: z.string().min(1),
  base: z.string().nullable(),
  ahead: commitCount.nullable(),
  behind: commitCount.nullable(),
  push: pushStatusSchema.nullable().default(null),
  commits: z.array(graphCommitSchema),
  // True when there are more commits than the list holds.
  truncated: z.boolean(),
})

export type GitGraph = z.infer<typeof gitGraphSchema>
