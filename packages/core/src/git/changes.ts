import { z } from 'zod'

// M modified, A added, D deleted, R renamed, U not tracked yet, C conflict.
export const CHANGE_STATUSES = ['M', 'A', 'D', 'R', 'U', 'C'] as const

export const changeStatusSchema = z.enum(CHANGE_STATUSES)
export type ChangeStatus = z.infer<typeof changeStatusSchema>

// A line count of one side of a change. Null when it is not known: binary, too big, or over the
// limit of files the daemon counts. Optional, so an older daemon without counts still parses.
const lineCountSchema = z.number().int().nonnegative().nullable().optional()

// One file that differs from the last commit of a workspace.
export const changeEntrySchema = z.object({
  path: z.string().min(1),
  status: changeStatusSchema,
  originalPath: z.string().nullable(),
  added: lineCountSchema,
  removed: lineCountSchema,
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

// The images the dashboard can show for a changed file, by file extension.
export const IMAGE_TYPES = {
  png: 'image/png',
  jpg: 'image/jpeg',
  jpeg: 'image/jpeg',
  gif: 'image/gif',
  webp: 'image/webp',
  svg: 'image/svg+xml',
} as const

export function imageTypeOf(path: string): string | null {
  const name = path.slice(path.lastIndexOf('/') + 1)
  const dot = name.lastIndexOf('.')
  if (dot === -1) {
    return null
  }
  const extension = name.slice(dot + 1).toLowerCase()
  return Object.hasOwn(IMAGE_TYPES, extension)
    ? IMAGE_TYPES[extension as keyof typeof IMAGE_TYPES]
    : null
}

// The last commit, or the file as it is now.
export const IMAGE_SIDES = ['original', 'modified'] as const
export type ImageSide = (typeof IMAGE_SIDES)[number]

// How much the workspace of a task differs from where its branch started, uncommitted changes
// included. Files that git does not track yet are not counted.
export const diffStatSchema = z.object({
  files: z.number().int().nonnegative(),
  added: z.number().int().nonnegative(),
  removed: z.number().int().nonnegative(),
})

export type DiffStat = z.infer<typeof diffStatSchema>

// One entry of a folder of the worktree, for the files explorer.
export const treeEntrySchema = z.object({
  name: z.string().min(1),
  kind: z.enum(['file', 'folder']),
})

export type TreeEntry = z.infer<typeof treeEntrySchema>

// A file of the worktree as the editor opens it. `text` is null when the file is binary or too
// large. `version` is the sha256 of the bytes: a save sends it back, so it never replaces a newer
// file.
export const workspaceFileSchema = z.object({
  path: z.string().min(1),
  text: z.string().nullable(),
  binary: z.boolean(),
  tooLarge: z.boolean(),
  version: z.string(),
})

export type WorkspaceFile = z.infer<typeof workspaceFileSchema>
