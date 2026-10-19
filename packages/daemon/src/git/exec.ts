import { execFile } from 'node:child_process'
import { promisify } from 'node:util'

const execFileOnce = promisify(execFile)
let spawns = 0

// How many git and gh processes this daemon started, for the benchmark. Not shown to users.
export function gitSpawnCount(): number {
  return spawns
}

export const execFileAsync = ((...args: Parameters<typeof execFileOnce>) => {
  spawns += 1
  return execFileOnce(...args)
}) as unknown as typeof execFileOnce

export const MAX_OUTPUT_BYTES = 10 * 1024 * 1024

export class GitError extends Error {
  readonly args: readonly string[]

  constructor(args: readonly string[], detail: string) {
    super(`git ${args[0] ?? ''} failed: ${detail}`)
    this.name = 'GitError'
    this.args = args
  }
}

export function errorText(error: unknown): string {
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

// Runs git with an argument list. Never builds a shell command. A timeout of 0 waits forever.
export async function git(
  cwd: string,
  args: readonly string[],
  { timeout = 0, env = {} }: { timeout?: number; env?: NodeJS.ProcessEnv } = {},
): Promise<string> {
  try {
    const { stdout } = await execFileAsync('git', [...args], {
      cwd,
      maxBuffer: MAX_OUTPUT_BYTES,
      env: { ...process.env, GIT_TERMINAL_PROMPT: '0', ...env },
      timeout,
    })
    return stdout
  } catch (error) {
    throw new GitError(args, errorText(error))
  }
}

export class OutsideWorkspaceError extends Error {
  constructor(path: string) {
    super(`${path} points outside the workspace, so it is not shown`)
    this.name = 'OutsideWorkspaceError'
  }
}

export async function gitBuffer(cwd: string, args: readonly string[]): Promise<Buffer> {
  try {
    const { stdout } = await execFileAsync('git', [...args], {
      cwd,
      maxBuffer: MAX_OUTPUT_BYTES,
      encoding: 'buffer',
      env: { ...process.env, GIT_TERMINAL_PROMPT: '0' },
    })
    return stdout
  } catch (error) {
    throw new GitError(args, errorText(error))
  }
}

// Adds settings through GIT_CONFIG_* variables, after the ones the environment already has.
export function withGitConfig(
  env: NodeJS.ProcessEnv,
  entries: readonly [key: string, value: string][],
): NodeJS.ProcessEnv {
  const start = Number(env.GIT_CONFIG_COUNT ?? '0') || 0
  const added: NodeJS.ProcessEnv = { GIT_CONFIG_COUNT: String(start + entries.length) }
  entries.forEach(([key, value], offset) => {
    added[`GIT_CONFIG_KEY_${start + offset}`] = key
    added[`GIT_CONFIG_VALUE_${start + offset}`] = value
  })
  return { ...env, ...added }
}

// The branch a checkout is on, or null when HEAD is detached.
export async function currentBranch(cwd: string): Promise<string | null> {
  try {
    const name = (await git(cwd, ['symbolic-ref', '--quiet', '--short', 'HEAD'])).trim()
    return name === '' ? null : name
  } catch (error) {
    if (error instanceof GitError) {
      return null
    }
    throw error
  }
}

// The top folder of the repository that holds `path`. Throws GitError when it is not in one.
export async function repositoryRoot(path: string): Promise<string> {
  return (await git(path, ['rev-parse', '--show-toplevel'])).trim()
}
