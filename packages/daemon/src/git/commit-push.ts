import { randomUUID } from 'node:crypto'
import { rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { errorText, execFileAsync, GitError, git, MAX_OUTPUT_BYTES, withGitConfig } from './exec.js'
import { FIELD } from './graph.js'

export class NothingToCommitError extends Error {
  constructor() {
    super('There is nothing to commit. The workspace matches its last commit.')
    this.name = 'NothingToCommitError'
  }
}

// Commits every change of the worktree, files that git does not track yet included.
export async function commitAll(
  worktreePath: string,
  message: string,
): Promise<{ hash: string; subject: string }> {
  await git(worktreePath, ['add', '--all'])
  const staged = await git(worktreePath, ['diff', '--cached', '--name-only'])
  if (staged.trim() === '') {
    throw new NothingToCommitError()
  }
  await git(worktreePath, ['commit', '--quiet', '-m', message])
  const [hash = '', subject = ''] = (await git(worktreePath, ['log', '-1', '--format=%H%x1f%s']))
    .trim()
    .split(FIELD)
  return { hash, subject }
}

// When git asks for a password for github.com, it asks the GitHub CLI, which uses GH_TOKEN or the
// login of `gh auth login`. Helpers that are set up already come first.
const GITHUB_CREDENTIALS: [string, string][] = [
  ['credential.https://github.com.helper', '!gh auth git-credential'],
]

// A push or a pull can take a while, but one that hangs must not hold the stage forever.
const PUSH_TIMEOUT_MS = 2 * 60 * 1000

async function remote(worktreePath: string, args: readonly string[]): Promise<void> {
  try {
    await execFileAsync('git', [...args], {
      cwd: worktreePath,
      maxBuffer: MAX_OUTPUT_BYTES,
      env: withGitConfig({ ...process.env, GIT_TERMINAL_PROMPT: '0' }, GITHUB_CREDENTIALS),
      timeout: PUSH_TIMEOUT_MS,
    })
  } catch (error) {
    throw new GitError(args, errorText(error))
  }
}

export async function pushBranch(worktreePath: string, branch: string): Promise<void> {
  await remote(worktreePath, ['push', '--set-upstream', 'origin', branch])
}

// Brings in the new commits of the branch's upstream, such as the merged pull requests of
// subtasks. False when the branch has no upstream. Fails when the two went different ways.
export async function pullFastForward(worktreePath: string): Promise<boolean> {
  try {
    await git(worktreePath, ['rev-parse', '--abbrev-ref', '@{upstream}'])
  } catch {
    return false
  }
  await remote(worktreePath, ['pull', '--ff-only', '--quiet'])
  return true
}

// A commit of the worktree as it is now, with the files that are not committed, made without
// touching its branch, its index or its files. Ignored files stay out.
export async function snapshotCommit(worktreePath: string, message: string): Promise<string> {
  const env = { GIT_INDEX_FILE: join(tmpdir(), `natamux-index-${randomUUID()}`) }
  try {
    await git(worktreePath, ['read-tree', 'HEAD'], { env })
    await git(worktreePath, ['add', '--all'], { env })
    const tree = (await git(worktreePath, ['write-tree'], { env })).trim()
    // A throwaway commit, so it needs no identity of the user's.
    const identity = ['-c', 'user.name=Natamux', '-c', 'user.email=natamux@localhost']
    const commit = [...identity, 'commit-tree', tree, '-p', 'HEAD', '-m', message]
    return (await git(worktreePath, commit)).trim()
  } finally {
    rmSync(env.GIT_INDEX_FILE, { force: true })
  }
}

// What `commitAll` would commit now: `--stat` in full, then the diff, cut so both fit in
// `maxChars`. Uses its own index, so the branch, the index and the files of the worktree stay as
// they are.
export async function readCommitDiff(worktreePath: string, maxChars: number): Promise<string> {
  const index = join(tmpdir(), `natamux-index-${randomUUID()}`)
  // Paths as they are, not in octal escapes, so the agent can read them.
  const env = withGitConfig({ ...process.env, GIT_INDEX_FILE: index }, [
    ['core.quotePath', 'false'],
  ])
  try {
    try {
      await git(worktreePath, ['read-tree', 'HEAD'], { env })
    } catch (error) {
      // A repository without a commit has no HEAD: everything is new.
      if (!(error instanceof GitError)) {
        throw error
      }
      await git(worktreePath, ['read-tree', '--empty'], { env })
    }
    await git(worktreePath, ['add', '--all'], { env })
    // Without HEAD, `diff --cached` shows every staged file.
    const diff = ['diff', '--cached', '--no-color', '--no-ext-diff', '-M']
    const stat = await git(worktreePath, [...diff, '--stat'], { env })
    if (stat.trim() === '') {
      throw new NothingToCommitError()
    }
    const full = await git(worktreePath, diff, { env })
    const room = Math.max(0, maxChars - stat.length - 1)
    if (full.length <= room) {
      return `${stat}\n${full}`
    }
    // Half of an emoji is not a character: cut before it.
    const end = /[\uDC00-\uDFFF]/.test(full[room] ?? '') ? Math.max(0, room - 1) : room
    return `${stat}\n${full.slice(0, end)}\n[diff cut: ${full.length - end} more characters]`
  } finally {
    rmSync(index, { force: true })
  }
}

// Puts a commit on a branch of origin, in place of what is there.
export async function pushRef(worktreePath: string, sha: string, ref: string): Promise<void> {
  await remote(worktreePath, ['push', '--force', '--quiet', 'origin', `${sha}:refs/heads/${ref}`])
}

export async function deleteRemoteRef(worktreePath: string, ref: string): Promise<void> {
  await remote(worktreePath, ['push', '--quiet', 'origin', '--delete', ref])
}
