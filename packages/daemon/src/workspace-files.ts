import { createHash } from 'node:crypto'
import { readdir, realpath, stat, writeFile } from 'node:fs/promises'
import { join, relative, sep } from 'node:path'
import type { TreeEntry, WorkspaceFile } from '@natamux/core'
import { looksBinary, MAX_FILE_BYTES, readInside } from './git.js'

export class WorkspacePathNotFoundError extends Error {
  constructor(path: string) {
    super(`${path} is not in the workspace`)
    this.name = 'WorkspacePathNotFoundError'
  }
}

export class WorkspacePathError extends Error {
  constructor(path: string, kind: 'file' | 'folder') {
    super(`${path === '' ? 'The workspace' : path} is not a ${kind}`)
    this.name = 'WorkspacePathError'
  }
}

// git keeps its own files there. Changing them by hand can break the worktree.
export class GitFolderError extends Error {
  constructor(path: string) {
    super(`${path} is inside .git, so it is not shown`)
    this.name = 'GitFolderError'
  }
}

export class FileChangedOnDiskError extends Error {
  constructor(path: string) {
    super(`${path} changed on disk after you opened it`)
    this.name = 'FileChangedOnDiskError'
  }
}

export class FileTooLargeError extends Error {
  constructor(path: string) {
    super(`${path} would be larger than ${MAX_FILE_BYTES / 1_000_000} MB, so it is not saved`)
    this.name = 'FileTooLargeError'
  }
}

function versionOf(bytes: Buffer | string): string {
  return createHash('sha256').update(bytes).digest('hex')
}

// Any case: macOS and Windows find `.GIT` too.
function isInGitFolder(realRoot: string, real: string): boolean {
  return relative(realRoot, real)
    .split(sep)
    .some((part) => part.toLowerCase() === '.git')
}

// Text that is not UTF-8 would come back changed after a save, so it is not opened as text.
function isText(bytes: Buffer): boolean {
  return !looksBinary(bytes) && Buffer.from(bytes.toString('utf8')).equals(bytes)
}

// The path in the worktree, with the guards of every explorer request: inside the worktree (links
// too), not in .git, and there.
async function resolveInside(root: string, path: string) {
  const found = await readInside(root, path)
  if (found === null) {
    throw new WorkspacePathNotFoundError(path)
  }
  if (isInGitFolder(await realpath(root), found.real)) {
    throw new GitFolderError(path)
  }
  return found
}

// One folder of the worktree, folders first. A link shows as what it points to; one that leaves
// the worktree or points nowhere is left out.
export async function listFolder(root: string, path: string): Promise<TreeEntry[]> {
  const folder = await resolveInside(root, path)
  if (!folder.isFolder) {
    throw new WorkspacePathError(path, 'folder')
  }
  const realRoot = await realpath(root)
  const listed = await readdir(folder.real, { withFileTypes: true })
  const entries = await Promise.all(
    listed.map(async (entry): Promise<TreeEntry | null> => {
      if (entry.name.toLowerCase() === '.git') {
        return null
      }
      if (entry.isDirectory()) {
        return { name: entry.name, kind: 'folder' }
      }
      if (entry.isFile()) {
        return { name: entry.name, kind: 'file' }
      }
      if (!entry.isSymbolicLink()) {
        return null
      }
      try {
        const real = await realpath(join(folder.real, entry.name))
        if (!real.startsWith(`${realRoot}${sep}`) || isInGitFolder(realRoot, real)) {
          return null
        }
        const stats = await stat(real)
        if (stats.isDirectory()) {
          return { name: entry.name, kind: 'folder' }
        }
        return stats.isFile() ? { name: entry.name, kind: 'file' } : null
      } catch {
        return null
      }
    }),
  )
  return entries
    .filter((entry) => entry !== null)
    .sort((a, b) =>
      a.kind === b.kind
        ? a.name.localeCompare(b.name, undefined, { numeric: true })
        : a.kind === 'folder'
          ? -1
          : 1,
    )
}

export async function readWorkspaceFile(root: string, path: string): Promise<WorkspaceFile> {
  const file = await resolveInside(root, path)
  if (!file.isFile) {
    throw new WorkspacePathError(path, 'file')
  }
  if (file.size > MAX_FILE_BYTES) {
    return { path, text: null, binary: false, tooLarge: true, version: '' }
  }
  const bytes = await file.read()
  const version = versionOf(bytes)
  if (!isText(bytes)) {
    return { path, text: null, binary: true, tooLarge: false, version }
  }
  return { path, text: bytes.toString('utf8'), binary: false, tooLarge: false, version }
}

// Only a file that is there and still has the bytes the editor read. Written in place, so the mode
// stays.
export async function writeWorkspaceFile(
  root: string,
  path: string,
  text: string,
  version: string,
): Promise<{ path: string; version: string }> {
  if (Buffer.byteLength(text) > MAX_FILE_BYTES) {
    throw new FileTooLargeError(path)
  }
  const file = await resolveInside(root, path)
  if (!file.isFile) {
    throw new WorkspacePathError(path, 'file')
  }
  if (versionOf(await file.read()) !== version) {
    throw new FileChangedOnDiskError(path)
  }
  await writeFile(file.real, text)
  return { path, version: versionOf(text) }
}
