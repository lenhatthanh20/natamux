import { existsSync } from 'node:fs'
import { realpath } from 'node:fs/promises'
import { basename, dirname, join, resolve } from 'node:path'
import { git } from './exec.js'
import { lines } from './graph.js'

// Makes a new branch for the worktree, from `baseRef` or HEAD. With `existing`, checks out the
// branch that is there instead, with the work it has. A new branch follows no branch: started
// from `origin/main`, it must not push to main.
export async function addWorktree(
  repoDir: string,
  worktreePath: string,
  branch: string,
  baseRef?: string,
  existing = false,
): Promise<void> {
  const base = baseRef === undefined ? [] : [baseRef]
  await git(
    repoDir,
    existing
      ? ['worktree', 'add', worktreePath, branch]
      : ['worktree', 'add', '--no-track', '-b', branch, worktreePath, ...base],
  )
}

// Refuses to remove a worktree that has changes that are not committed, or submodules checked
// out. `force` removes it either way.
export async function removeWorktree(
  repoDir: string,
  worktreePath: string,
  force = false,
): Promise<void> {
  await git(repoDir, ['worktree', 'remove', ...(force ? ['--force'] : []), worktreePath])
}

// Whether a submodule is checked out in the worktree. Lines of submodules that are not start
// with "-".
export async function hasSubmodules(worktreePath: string): Promise<boolean> {
  const output = await git(worktreePath, ['submodule', 'status'])
  return lines(output).some((line) => !line.startsWith('-'))
}

// Reads the output of `git worktree list --porcelain`: the path of each worktree, main first.
export function parseWorktreeList(output: string): string[] {
  const prefix = 'worktree '
  return output
    .split('\n')
    .filter((line) => line.startsWith(prefix))
    .map((line) => line.slice(prefix.length))
}

export async function listWorktreePaths(repoDir: string): Promise<string[]> {
  return parseWorktreeList(await git(repoDir, ['worktree', 'list', '--porcelain']))
}

// stale: git lists the worktree but its folder is gone. missing: git does not list it and the
// folder is gone. not-a-worktree: the folder is there but git does not know it.
export type WorktreeState = 'present' | 'stale' | 'missing' | 'not-a-worktree'

// git lists real paths, so a path is resolved the same way before it is compared. A folder that
// is gone is resolved through its parent.
async function comparablePath(path: string): Promise<string> {
  try {
    return await realpath(path)
  } catch {
    try {
      return join(await realpath(dirname(path)), basename(path))
    } catch {
      return resolve(path)
    }
  }
}

export async function worktreeState(repoDir: string, worktreePath: string): Promise<WorktreeState> {
  const exists = existsSync(worktreePath)
  const wanted = await comparablePath(worktreePath)
  const listed = await Promise.all((await listWorktreePaths(repoDir)).map(comparablePath))
  const registered = listed.includes(wanted)
  if (exists) {
    return registered ? 'present' : 'not-a-worktree'
  }
  return registered ? 'stale' : 'missing'
}
