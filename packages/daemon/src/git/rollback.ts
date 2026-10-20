import type { ChangeEntry } from '@natamux/core'
import { GitError, git } from './exec.js'

// A file named like `a*.ts` or `:(top)x` names only itself, never a pattern of other files.
const LITERAL = { env: { GIT_LITERAL_PATHSPECS: '1' } }
// Paths go on the command line, which the system limits (1 MiB on macOS, environment included).
const MAX_BATCH_CHARS = 64 * 1024

// The paths in groups whose length stays under MAX_BATCH_CHARS. A group has one path at least.
export function pathBatches(paths: readonly string[]): string[][] {
  const batches: string[][] = []
  let batch: string[] = []
  let chars = 0
  for (const path of paths) {
    if (batch.length > 0 && chars + path.length + 1 > MAX_BATCH_CHARS) {
      batches.push(batch)
      batch = []
      chars = 0
    }
    batch.push(path)
    chars += path.length + 1
  }
  if (batch.length > 0) {
    batches.push(batch)
  }
  return batches
}

async function eachBatch(
  worktreePath: string,
  args: readonly string[],
  paths: readonly string[],
): Promise<void> {
  for (const batch of pathBatches(paths)) {
    await git(worktreePath, [...args, '--', ...batch], LITERAL)
  }
}

export async function hasCommit(worktreePath: string): Promise<boolean> {
  try {
    await git(worktreePath, ['rev-parse', '--verify', '--quiet', 'HEAD^{commit}'])
    return true
  } catch (error) {
    if (error instanceof GitError) {
      return false
    }
    throw error
  }
}

// Every path a rollback of these files touches: a rename has its old path too.
export function rollbackPaths(entries: readonly ChangeEntry[]): string[] {
  return entries.flatMap((entry) =>
    entry.status === 'R' && entry.originalPath !== null
      ? [entry.originalPath, entry.path]
      : [entry.path],
  )
}

// Puts the files back as they are in HEAD, in the index and on disk. A file that HEAD does not
// have is deleted. `git clean` deletes only files git does not track, never ignored ones.
export async function rollbackFiles(
  worktreePath: string,
  entries: readonly ChangeEntry[],
): Promise<void> {
  const untracked = entries.filter((entry) => entry.status === 'U')
  const tracked = entries.filter((entry) => entry.status !== 'U')
  await eachBatch(
    worktreePath,
    ['restore', '--source=HEAD', '--staged', '--worktree'],
    rollbackPaths(tracked),
  )
  await eachBatch(
    worktreePath,
    ['clean', '-f', '-q'],
    untracked.map((entry) => entry.path),
  )
}

// Undoes a rollback: each path gets its file from the snapshot made before it. A path that the
// snapshot does not have (a deleted file, the old name of a renamed one) is deleted again. Only
// the files come back, not the index: a commit adds every file anyway.
export async function restoreFromSnapshot(
  worktreePath: string,
  snapshot: string,
  paths: readonly string[],
): Promise<void> {
  const kept = new Set<string>()
  for (const batch of pathBatches(paths)) {
    const args = ['ls-tree', '-r', '-z', '--name-only', snapshot, '--', ...batch]
    for (const path of (await git(worktreePath, args, LITERAL)).split('\0')) {
      if (path !== '') {
        kept.add(path)
      }
    }
  }
  await eachBatch(
    worktreePath,
    ['restore', `--source=${snapshot}`, '--worktree'],
    paths.filter((path) => kept.has(path)),
  )
  await eachBatch(
    worktreePath,
    ['rm', '-q', '-f', '--ignore-unmatch'],
    paths.filter((path) => !kept.has(path)),
  )
}
