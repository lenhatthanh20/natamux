import { readFile, realpath, stat } from 'node:fs/promises'
import { resolve, sep } from 'node:path'
import { type ChangeEntry, type FileDiff, type ImageSide, imageTypeOf } from '@natamux/core'
import { git, gitBuffer, OutsideWorkspaceError } from './exec.js'

// A file bigger than this is not sent to the dashboard, and the text of a file is binary when a
// zero byte shows up in its first bytes.
export const MAX_FILE_BYTES = 1_000_000
const BINARY_SCAN_BYTES = 8000

export function looksBinary(buffer: Buffer): boolean {
  return buffer.subarray(0, BINARY_SCAN_BYTES).includes(0)
}

export type FileInWorktree = {
  real: string
  isFile: boolean
  isFolder: boolean
  size: number
  mtimeMs: number
  read: () => Promise<Buffer>
}

// The file in the worktree, or null when it is not there. A link that leaves the worktree is refused.
export async function readInside(root: string, path: string): Promise<FileInWorktree | null> {
  let real: string
  try {
    real = await realpath(resolve(root, path))
  } catch (error) {
    if (typeof error === 'object' && error !== null && 'code' in error && error.code === 'ENOENT') {
      return null
    }
    throw error
  }

  const realRoot = await realpath(root)
  if (real !== realRoot && !real.startsWith(`${realRoot}${sep}`)) {
    throw new OutsideWorkspaceError(path)
  }
  const stats = await stat(real)
  return {
    real,
    isFile: stats.isFile(),
    isFolder: stats.isDirectory(),
    size: stats.size,
    mtimeMs: stats.mtimeMs,
    read: () => readFile(real),
  }
}

// Both sides of one changed file: the last commit, and the file as it is now.
export async function readFileDiff(worktreePath: string, change: ChangeEntry): Promise<FileDiff> {
  const head = { path: change.path, status: change.status }
  const tooLarge = { ...head, original: null, modified: null, binary: false, tooLarge: true }

  let originalBuffer: Buffer | null = null
  if (change.status !== 'A' && change.status !== 'U') {
    const source = `HEAD:${change.originalPath ?? change.path}`
    const size = Number((await git(worktreePath, ['cat-file', '-s', source])).trim())
    if (size > MAX_FILE_BYTES) {
      return tooLarge
    }
    originalBuffer = await gitBuffer(worktreePath, ['show', source])
  }

  let modifiedBuffer: Buffer | null = null
  if (change.status !== 'D') {
    const file = await readInside(worktreePath, change.path)
    if (file) {
      if (file.size > MAX_FILE_BYTES) {
        return tooLarge
      }
      modifiedBuffer = await file.read()
    }
  }

  if (
    (originalBuffer && looksBinary(originalBuffer)) ||
    (modifiedBuffer && looksBinary(modifiedBuffer))
  ) {
    return { ...head, original: null, modified: null, binary: true, tooLarge: false }
  }
  return {
    ...head,
    original: originalBuffer?.toString('utf8') ?? null,
    modified: modifiedBuffer?.toString('utf8') ?? null,
    binary: false,
    tooLarge: false,
  }
}

// Images are sent as they are, so the limit is higher than for text.
export const MAX_IMAGE_BYTES = 5_000_000

export class NotAnImageError extends Error {
  constructor(path: string) {
    super(`${path} is not an image that the dashboard can show`)
    this.name = 'NotAnImageError'
  }
}

export class ImageSideMissingError extends Error {
  constructor(path: string, side: ImageSide) {
    super(
      side === 'original'
        ? `${path} is not in the last commit`
        : `${path} is not in the workspace any more`,
    )
    this.name = 'ImageSideMissingError'
  }
}

export class ImageTooLargeError extends Error {
  constructor(path: string) {
    super(`${path} is larger than ${MAX_IMAGE_BYTES / 1_000_000} MB, so it is not shown`)
    this.name = 'ImageTooLargeError'
  }
}

// One side of a changed image: the last commit (`original`) or the file now (`modified`).
export async function readFileImage(
  worktreePath: string,
  change: ChangeEntry,
  side: ImageSide,
): Promise<{ type: string; bytes: Buffer }> {
  const type = imageTypeOf(change.path)
  if (type === null) {
    throw new NotAnImageError(change.path)
  }

  if (side === 'original') {
    const source = change.originalPath ?? change.path
    // A file renamed from something that is not an image has no old image to show.
    const originalType = imageTypeOf(source)
    if (change.status === 'A' || change.status === 'U' || originalType === null) {
      throw new ImageSideMissingError(change.path, side)
    }
    const size = Number((await git(worktreePath, ['cat-file', '-s', `HEAD:${source}`])).trim())
    if (size > MAX_IMAGE_BYTES) {
      throw new ImageTooLargeError(change.path)
    }
    return { type: originalType, bytes: await gitBuffer(worktreePath, ['show', `HEAD:${source}`]) }
  }

  if (change.status === 'D') {
    throw new ImageSideMissingError(change.path, side)
  }
  const file = await readInside(worktreePath, change.path)
  if (file === null) {
    throw new ImageSideMissingError(change.path, side)
  }
  if (file.size > MAX_IMAGE_BYTES) {
    throw new ImageTooLargeError(change.path)
  }
  return { type, bytes: await file.read() }
}
