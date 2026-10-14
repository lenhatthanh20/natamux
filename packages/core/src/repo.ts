import { z } from 'zod'

// The id of the repository that tasks without one belong to. It can be removed like any other.
export const DEFAULT_REPO = 'default'

export const repoIdSchema = z
  .string()
  .regex(/^[a-z0-9]+(-[a-z0-9]+)*$/, 'A repository id is lower case words joined by "-"')
  .max(60)

// A git repository whose tasks Natamux manages.
export const repoSchema = z.object({
  id: repoIdSchema,
  name: z.string().trim().min(1).max(100),
  path: z.string().min(1),
  // Where the worktrees of its tasks go. Left out: the daemon's worktree folder.
  worktreesDir: z
    .string()
    .regex(/^(\/|[A-Za-z]:[\\/]|\\\\)/, 'The worktree folder must be an absolute path')
    .optional(),
})

export type Repo = z.infer<typeof repoSchema>
