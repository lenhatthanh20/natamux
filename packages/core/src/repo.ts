import { z } from 'zod'

// The repository the daemon was started for. It is always there and cannot be removed.
export const DEFAULT_REPO = 'default'

export const repoIdSchema = z
  .string()
  .regex(/^[a-z0-9]+(-[a-z0-9]+)*$/, 'A repository id is lower case words joined by "-"')
  .max(60)

// A git repository whose tasks Natamux manages. Its worktrees go to the daemon's worktree folder.
export const repoSchema = z.object({
  id: repoIdSchema,
  name: z.string().trim().min(1).max(100),
  path: z.string().min(1),
})

export type Repo = z.infer<typeof repoSchema>
