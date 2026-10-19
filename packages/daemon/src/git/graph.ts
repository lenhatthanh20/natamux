import type { GitGraph, GraphCommit, PushStatus } from '@natamux/core'
import { currentBranch, GitError, git } from './exec.js'

// The graph shows at most this many commits. Older ones are left out.
export const MAX_GRAPH_COMMITS = 200

export const FIELD = '\x1f'
const LOG_FORMAT = ['%H', '%P', '%an', '%aI', '%D', '%s'].join('%x1f')

// The remote branch that `branch` follows, such as "origin/main". Null without one.
async function upstreamOf(repoDir: string, branch: string): Promise<string | null> {
  try {
    const args = ['rev-parse', '--abbrev-ref', '--symbolic-full-name', `${branch}@{upstream}`]
    const name = (await git(repoDir, args)).trim()
    return name === '' ? null : name
  } catch (error) {
    if (error instanceof GitError) {
      return null
    }
    throw error
  }
}

// Where new work from `branch` starts: its remote branch when the local branch is behind it, so
// a branch that was not pulled does not start tasks from old code. Local commits that are not
// pushed win. Null: the local branch.
export async function startRef(repoDir: string, branch: string): Promise<string | null> {
  const upstream = await upstreamOf(repoDir, branch)
  if (upstream === null) {
    return null
  }
  try {
    await git(repoDir, ['merge-base', '--is-ancestor', branch, upstream])
    return upstream
  } catch (error) {
    if (error instanceof GitError) {
      return null
    }
    throw error
  }
}

// A fetch that hangs (no network) must not hold up a new workspace for long.
const FETCH_TIMEOUT_MS = 20_000

// Brings the remote branch of `branch` up to date. The local branch does not move. Without a
// network, or a remote, the work starts from what is known, so a failure is not an error.
export async function fetchUpstream(repoDir: string, branch: string): Promise<void> {
  try {
    const remote = (await git(repoDir, ['config', '--get', `branch.${branch}.remote`])).trim()
    const merge = (await git(repoDir, ['config', '--get', `branch.${branch}.merge`])).trim()
    await git(repoDir, ['fetch', '--quiet', remote, merge], { timeout: FETCH_TIMEOUT_MS })
  } catch (error) {
    if (!(error instanceof GitError)) {
      throw error
    }
  }
}

// "HEAD -> natamux/t-0001, tag: v1, main" becomes ["natamux/t-0001", "tag: v1", "main"].
// "origin/HEAD" is left out: it only repeats the default branch of the remote.
export function parseRefs(decoration: string): string[] {
  return decoration
    .split(', ')
    .map((ref) => ref.trim())
    .filter((ref) => ref !== '' && ref !== 'HEAD' && !ref.endsWith('/HEAD'))
    .map((ref) => (ref.startsWith('HEAD -> ') ? ref.slice('HEAD -> '.length) : ref))
}

// Reads `git log -z` in LOG_FORMAT: one NUL-ended record per commit, fields split by FIELD.
export function parseLog(output: string, isOnBranch: (hash: string) => boolean): GraphCommit[] {
  const commits: GraphCommit[] = []
  for (const record of output.split('\0')) {
    const fields = record.replace(/^\n/, '').split(FIELD)
    const [hash, parents, author, date, decoration, ...subject] = fields
    if (!hash || fields.length < 6) {
      continue
    }
    commits.push({
      hash,
      parents: parents ? parents.split(' ') : [],
      author: author ?? '',
      date: date ?? '',
      subject: subject.join(FIELD),
      refs: parseRefs(decoration ?? ''),
      onBranch: isOnBranch(hash),
    })
  }
  return commits
}

export function lines(output: string): string[] {
  return output.split('\n').filter((line) => line !== '')
}

// Whether origin has `branch`, by the remote-tracking ref that a push or a fetch left. No network.
export async function hasRemoteBranch(worktreePath: string, branch: string): Promise<boolean> {
  const ref = `refs/remotes/origin/${branch}`
  return lines(await git(worktreePath, ['for-each-ref', '--format=%(refname)', ref])).includes(ref)
}

// Compares `branch` with its copy on origin. Uses the remote-tracking refs as they are: no fetch.
// Not `@{upstream}`, because a branch made from origin/main tracks origin/main.
export async function readPushStatus(worktreePath: string, branch: string): Promise<PushStatus> {
  if (!lines(await git(worktreePath, ['remote'])).includes('origin')) {
    return { remote: 'none' }
  }
  const remoteRef = `refs/remotes/origin/${branch}`
  const found = lines(await git(worktreePath, ['for-each-ref', '--format=%(refname)', remoteRef]))
  if (!found.includes(remoteRef)) {
    const count = await git(worktreePath, [
      'rev-list',
      '--count',
      'HEAD',
      '--not',
      '--remotes=origin',
      '--',
    ])
    return { remote: 'new', unpushed: Number(count.trim()) }
  }
  const counts = await git(worktreePath, [
    'rev-list',
    '--left-right',
    '--count',
    `HEAD...${remoteRef}`,
    '--',
  ])
  const [left, right] = counts.trim().split(/\s+/)
  return { remote: 'tracked', unpushed: Number(left), unpulled: Number(right) }
}

// The history of the worktree's branch and of `base`, newest first, in the order git draws a graph.
export async function readGitGraph(
  worktreePath: string,
  base: string | null,
  limit = MAX_GRAPH_COMMITS,
): Promise<GitGraph> {
  const head = (await git(worktreePath, ['rev-parse', 'HEAD'])).trim()
  const branch = await currentBranch(worktreePath)
  const compareTo = base === branch ? null : base

  const log = await git(worktreePath, [
    'log',
    '-z',
    '--topo-order',
    `--max-count=${limit + 1}`,
    `--format=${LOG_FORMAT}`,
    'HEAD',
    ...(compareTo === null ? [] : [compareTo]),
    '--',
  ])

  let ahead: number | null = null
  let behind: number | null = null
  let isOnBranch = (_hash: string) => true
  if (compareTo !== null) {
    const counts = await git(worktreePath, [
      'rev-list',
      '--left-right',
      '--count',
      `HEAD...${compareTo}`,
      '--',
    ])
    const [left, right] = counts.trim().split(/\s+/)
    ahead = Number(left)
    behind = Number(right)
    const own = new Set(
      lines(
        await git(worktreePath, [
          'rev-list',
          `--max-count=${limit + 1}`,
          `${compareTo}..HEAD`,
          '--',
        ]),
      ),
    )
    isOnBranch = (hash) => own.has(hash)
  }

  const commits = parseLog(log, isOnBranch)
  return {
    branch,
    head,
    base: compareTo,
    ahead,
    behind,
    push: branch === null ? null : await readPushStatus(worktreePath, branch),
    commits: commits.slice(0, limit),
    truncated: commits.length > limit,
  }
}
