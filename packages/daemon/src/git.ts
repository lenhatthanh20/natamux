import { execFile } from 'node:child_process'
import { promisify } from 'node:util'

const execFileAsync = promisify(execFile)

const MAX_OUTPUT_BYTES = 10 * 1024 * 1024

export class GitError extends Error {
  readonly args: readonly string[]

  constructor(args: readonly string[], detail: string) {
    super(`git ${args[0] ?? ''} failed: ${detail}`)
    this.name = 'GitError'
    this.args = args
  }
}

function errorText(error: unknown): string {
  if (typeof error === 'object' && error !== null) {
    if ('code' in error && error.code === 'ENOENT') {
      return 'git is not installed or not on the PATH'
    }
    if ('stderr' in error && typeof error.stderr === 'string' && error.stderr.trim() !== '') {
      return error.stderr.trim()
    }
  }
  return error instanceof Error ? error.message : String(error)
}

// Runs git with an argument list. Never builds a shell command.
async function git(cwd: string, args: readonly string[]): Promise<string> {
  try {
    const { stdout } = await execFileAsync('git', [...args], {
      cwd,
      maxBuffer: MAX_OUTPUT_BYTES,
      env: { ...process.env, GIT_TERMINAL_PROMPT: '0' },
    })
    return stdout
  } catch (error) {
    throw new GitError(args, errorText(error))
  }
}

export async function addWorktree(
  repoDir: string,
  worktreePath: string,
  branch: string,
  baseRef?: string,
): Promise<void> {
  await git(repoDir, [
    'worktree',
    'add',
    '-b',
    branch,
    worktreePath,
    ...(baseRef === undefined ? [] : [baseRef]),
  ])
}

// Refuses to remove a worktree that has changes that are not committed.
export async function removeWorktree(repoDir: string, worktreePath: string): Promise<void> {
  await git(repoDir, ['worktree', 'remove', worktreePath])
}

// Deletes the branch only when it is fully merged. Returns false when git keeps it.
export async function deleteMergedBranch(repoDir: string, branch: string): Promise<boolean> {
  try {
    await git(repoDir, ['branch', '-d', branch])
    return true
  } catch (error) {
    if (error instanceof GitError) {
      return false
    }
    throw error
  }
}
