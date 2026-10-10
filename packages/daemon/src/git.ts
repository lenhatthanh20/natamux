import { execFile } from 'node:child_process'
import { existsSync } from 'node:fs'
import { readFile, realpath, stat } from 'node:fs/promises'
import { basename, dirname, join, resolve, sep } from 'node:path'
import { promisify } from 'node:util'
import {
  type ChangeEntry,
  type ChangeStatus,
  type DiffStat,
  type FileDiff,
  type GitGraph,
  type GraphCommit,
  type ImageSide,
  imageTypeOf,
  type PullRequestStatus,
} from '@natamux/core'

const execFileAsync = promisify(execFile)

const MAX_OUTPUT_BYTES = 10 * 1024 * 1024

// A file bigger than this is not sent to the dashboard, and the text of a file is binary when a
// zero byte shows up in its first bytes.
const MAX_FILE_BYTES = 1_000_000
const BINARY_SCAN_BYTES = 8000

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

// Makes a new branch for the worktree, from `baseRef` or HEAD. With `existing`, checks out the
// branch that is there instead, with the work it has.
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
      : ['worktree', 'add', '-b', branch, worktreePath, ...base],
  )
}

// Refuses to remove a worktree that has changes that are not committed.
export async function removeWorktree(repoDir: string, worktreePath: string): Promise<void> {
  await git(repoDir, ['worktree', 'remove', worktreePath])
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

export class OutsideWorkspaceError extends Error {
  constructor(path: string) {
    super(`${path} points outside the workspace, so it is not shown`)
    this.name = 'OutsideWorkspaceError'
  }
}

async function gitBuffer(cwd: string, args: readonly string[]): Promise<Buffer> {
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

export function statusFromCodes(index: string, worktree: string): ChangeStatus {
  const conflict =
    index === 'U' ||
    worktree === 'U' ||
    (index === 'A' && worktree === 'A') ||
    (index === 'D' && worktree === 'D')
  if (conflict) {
    return 'C'
  }
  if (index === '?' && worktree === '?') {
    return 'U'
  }
  if (index === 'R' || worktree === 'R') {
    return 'R'
  }
  if (index === 'A' || worktree === 'A' || index === 'C' || worktree === 'C') {
    return 'A'
  }
  if (index === 'D' || worktree === 'D') {
    return 'D'
  }
  return 'M'
}

// Reads the output of `git status --porcelain=v1 -z`. Each entry is "XY path" and a NUL; a renamed
// or copied file is followed by one more NUL-ended item, its old path.
export function parseStatus(output: string): ChangeEntry[] {
  const parts = output.split('\0')
  const entries: ChangeEntry[] = []

  for (let position = 0; position < parts.length; position += 1) {
    const item = parts[position]
    if (!item || item.length < 4) {
      continue
    }
    const index = item[0] ?? ' '
    const worktree = item[1] ?? ' '
    const moved = index === 'R' || index === 'C' || worktree === 'R' || worktree === 'C'
    const originalPath = moved ? (parts[position + 1] ?? null) : null
    if (moved) {
      position += 1
    }
    entries.push({
      path: item.slice(3),
      status: statusFromCodes(index, worktree),
      originalPath,
    })
  }
  return entries
}

type LineCounts = { added: number | null; removed: number | null }

// Reads `git diff --numstat -z`. A record is "added<TAB>removed<TAB>path" and a NUL. For a
// rename the path is empty and two NUL-ended items follow, the old path and the new one. Binary
// files have "-" for both counts. Keyed by the path the file has now.
export function parseNumstatZ(output: string): Map<string, LineCounts> {
  const parts = output.split('\0')
  const counts = new Map<string, LineCounts>()
  for (let position = 0; position < parts.length; position += 1) {
    const item = parts[position]
    if (!item) {
      continue
    }
    const firstTab = item.indexOf('\t')
    const secondTab = item.indexOf('\t', firstTab + 1)
    if (firstTab === -1 || secondTab === -1) {
      continue
    }
    const added = item.slice(0, firstTab)
    const removed = item.slice(firstTab + 1, secondTab)
    let path = item.slice(secondTab + 1)
    if (path === '') {
      path = parts[position + 2] ?? ''
      position += 2
    }
    if (path === '') {
      continue
    }
    counts.set(path, {
      added: added === '-' ? null : Number(added),
      removed: removed === '-' ? null : Number(removed),
    })
  }
  return counts
}

// The lines of a text file: one for each line break, and one more for a last line without one.
export function countTextLines(buffer: Buffer): number {
  let lines = 0
  for (let at = buffer.indexOf(0x0a); at !== -1; at = buffer.indexOf(0x0a, at + 1)) {
    lines += 1
  }
  return buffer.length > 0 && buffer[buffer.length - 1] !== 0x0a ? lines + 1 : lines
}

// Reading every new file on each refresh would be slow, so only this many are counted.
const MAX_COUNTED_UNTRACKED = 300
const MAX_CACHED_COUNTS = 5000
// The line count of an untracked file, by its real path. It is used while the size and the time
// of the last change are the same.
const untrackedLines = new Map<string, { size: number; mtimeMs: number; lines: number | null }>()

async function untrackedCounts(worktreePath: string, path: string): Promise<LineCounts> {
  let file: FileInWorktree | null
  try {
    file = await readInside(worktreePath, path)
  } catch {
    return { added: null, removed: null }
  }
  // Git lists a nested repository as one untracked folder.
  if (file === null || !file.isFile || file.size > MAX_FILE_BYTES) {
    return { added: null, removed: null }
  }
  const cached = untrackedLines.get(file.real)
  let lines: number | null
  if (cached !== undefined && cached.size === file.size && cached.mtimeMs === file.mtimeMs) {
    lines = cached.lines
  } else {
    let buffer: Buffer
    try {
      buffer = await file.read()
    } catch {
      // An agent can delete the file after the stat. One file must not fail the whole list.
      return { added: null, removed: null }
    }
    lines = looksBinary(buffer) ? null : countTextLines(buffer)
    if (untrackedLines.size >= MAX_CACHED_COUNTS) {
      untrackedLines.clear()
    }
    untrackedLines.set(file.real, { size: file.size, mtimeMs: file.mtimeMs, lines })
  }
  return lines === null ? { added: null, removed: null } : { added: lines, removed: 0 }
}

async function lineCounts(
  worktreePath: string,
  entries: readonly ChangeEntry[],
): Promise<ChangeEntry[]> {
  let tracked = new Map<string, LineCounts>()
  try {
    // `-M` finds renames even when the user's git config turns them off.
    tracked = parseNumstatZ(
      await git(worktreePath, ['diff', '--numstat', '-z', '-M', 'HEAD', '--']),
    )
  } catch (error) {
    // A repository without a commit has no HEAD to compare with.
    if (!(error instanceof GitError)) {
      throw error
    }
  }
  let untrackedSeen = 0
  return Promise.all(
    entries.map(async (entry) => {
      if (entry.status === 'U') {
        untrackedSeen += 1
        if (untrackedSeen <= MAX_COUNTED_UNTRACKED) {
          return { ...entry, ...(await untrackedCounts(worktreePath, entry.path)) }
        }
        return { ...entry, added: null, removed: null }
      }
      return { ...entry, ...(tracked.get(entry.path) ?? { added: null, removed: null }) }
    }),
  )
}

// The files of a worktree that differ from its last commit, including files not tracked yet. With
// `counts`, each file also has the lines it adds and removes. Without, it costs one git call.
export async function listChanges(
  worktreePath: string,
  options: { counts?: boolean } = {},
): Promise<ChangeEntry[]> {
  const output = await git(worktreePath, [
    'status',
    '--porcelain=v1',
    '-z',
    '--untracked-files=all',
  ])
  const entries = parseStatus(output).sort((a, b) => a.path.localeCompare(b.path))
  return options.counts === true ? lineCounts(worktreePath, entries) : entries
}

function looksBinary(buffer: Buffer): boolean {
  return buffer.subarray(0, BINARY_SCAN_BYTES).includes(0)
}

type FileInWorktree = {
  real: string
  isFile: boolean
  size: number
  mtimeMs: number
  read: () => Promise<Buffer>
}

// The file in the worktree, or null when it is not there. A link that leaves the worktree is refused.
async function readInside(root: string, path: string): Promise<FileInWorktree | null> {
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

// The graph shows at most this many commits. Older ones are left out.
export const MAX_GRAPH_COMMITS = 200

const FIELD = '\x1f'
const LOG_FORMAT = ['%H', '%P', '%an', '%aI', '%D', '%s'].join('%x1f')

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

function lines(output: string): string[] {
  return output.split('\n').filter((line) => line !== '')
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
    commits: commits.slice(0, limit),
    truncated: commits.length > limit,
  }
}

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

// When git asks for a password for github.com, it asks the GitHub CLI, which uses GH_TOKEN or the
// login of `gh auth login`. Helpers that are set up already come first.
const GITHUB_CREDENTIALS: [string, string][] = [
  ['credential.https://github.com.helper', '!gh auth git-credential'],
]

export async function pushBranch(worktreePath: string, branch: string): Promise<void> {
  const args = ['push', '--set-upstream', 'origin', branch]
  try {
    await execFileAsync('git', args, {
      cwd: worktreePath,
      maxBuffer: MAX_OUTPUT_BYTES,
      env: withGitConfig({ ...process.env, GIT_TERMINAL_PROMPT: '0' }, GITHUB_CREDENTIALS),
    })
  } catch (error) {
    throw new GitError(args, errorText(error))
  }
}

export class GitHubError extends Error {
  constructor(detail: string) {
    super(`GitHub CLI failed: ${detail}`)
    this.name = 'GitHubError'
  }
}

export type RunGh = (cwd: string, args: readonly string[]) => Promise<string>

// Runs the GitHub CLI without prompts. Its errors become GitHubError.
export const runGh: RunGh = async (cwd, args) => {
  try {
    const { stdout } = await execFileAsync('gh', [...args], {
      cwd,
      maxBuffer: MAX_OUTPUT_BYTES,
      env: { ...process.env, GH_PROMPT_DISABLED: '1', GH_NO_UPDATE_NOTIFIER: '1', NO_COLOR: '1' },
    })
    return stdout
  } catch (error) {
    if (typeof error === 'object' && error !== null && 'code' in error && error.code === 'ENOENT') {
      throw new GitHubError('gh is not installed or not on the PATH')
    }
    throw new GitHubError(errorText(error))
  }
}

const PULL_REQUEST_URL = /https:\/\/\S+\/pull\/\d+/

// The address of a pull request in the output of `gh pr create`, or undefined.
export function pullRequestUrl(output: string): string | undefined {
  return PULL_REQUEST_URL.exec(output)?.[0]
}

// Opens a pull request from `branch` into the default branch of the repository. When one is open
// for the branch already, gives its address instead.
export async function createPullRequest(
  worktreePath: string,
  request: { branch: string; title: string; body: string },
  gh: RunGh = runGh,
): Promise<string> {
  let output: string
  try {
    output = await gh(worktreePath, [
      'pr',
      'create',
      '--head',
      request.branch,
      '--title',
      request.title,
      '--body',
      request.body,
    ])
  } catch (error) {
    const existing = error instanceof GitHubError ? pullRequestUrl(error.message) : undefined
    if (existing && /already exists/i.test(error instanceof Error ? error.message : '')) {
      return existing
    }
    throw error
  }
  const url = pullRequestUrl(output)
  if (!url) {
    throw new GitHubError(`no pull request address in the answer: ${output.trim()}`)
  }
  return url
}

type CheckItem = { status?: string; conclusion?: string; state?: string }

const FAILED_CONCLUSIONS = [
  'FAILURE',
  'TIMED_OUT',
  'CANCELLED',
  'ACTION_REQUIRED',
  'STARTUP_FAILURE',
]

// One word for all checks of a pull request. `statusCheckRollup` of `gh pr view` mixes check
// runs (status and conclusion) and commit statuses (state).
export function summarizeChecks(items: readonly CheckItem[]): PullRequestStatus['checks'] {
  if (items.length === 0) {
    return 'none'
  }
  const failed = items.some(
    (item) =>
      FAILED_CONCLUSIONS.includes(item.conclusion ?? '') ||
      item.state === 'FAILURE' ||
      item.state === 'ERROR',
  )
  if (failed) {
    return 'failing'
  }
  const waiting = items.some((item) =>
    item.state === undefined
      ? item.status !== 'COMPLETED'
      : item.state === 'PENDING' || item.state === 'EXPECTED',
  )
  return waiting ? 'pending' : 'passing'
}

// Reads the output of `gh pr view --json state,isDraft,statusCheckRollup`.
export function parsePullRequestView(output: string): PullRequestStatus {
  const data = JSON.parse(output) as {
    state?: string
    isDraft?: boolean
    statusCheckRollup?: CheckItem[] | null
  }
  const state =
    data.state === 'MERGED'
      ? 'merged'
      : data.state === 'CLOSED'
        ? 'closed'
        : data.isDraft
          ? 'draft'
          : 'open'
  return { state, checks: summarizeChecks(data.statusCheckRollup ?? []) }
}

export async function readPullRequestStatus(
  cwd: string,
  url: string,
  gh: RunGh = runGh,
): Promise<PullRequestStatus> {
  const output = await gh(cwd, ['pr', 'view', url, '--json', 'state,isDraft,statusCheckRollup'])
  try {
    return parsePullRequestView(output)
  } catch {
    throw new GitHubError(`could not read the pull request: ${output.slice(0, 200)}`)
  }
}

// Sums `git diff --numstat`. Binary files count as files, without lines.
export function parseNumstat(output: string): DiffStat {
  const stat: DiffStat = { files: 0, added: 0, removed: 0 }
  for (const line of output.split('\n')) {
    const [added, removed] = line.split('\t')
    if (added === undefined || removed === undefined) {
      continue
    }
    stat.files += 1
    stat.added += Number(added) || 0
    stat.removed += Number(removed) || 0
  }
  return stat
}

// The worktree against the commit where its branch left `base`, or against HEAD without a base.
export async function readDiffStat(worktreePath: string, base: string | null): Promise<DiffStat> {
  const from =
    base === null ? 'HEAD' : (await git(worktreePath, ['merge-base', 'HEAD', base])).trim()
  return parseNumstat(await git(worktreePath, ['diff', '--numstat', from, '--']))
}

// The top folder of the repository that holds `path`. Throws GitError when it is not in one.
export async function repositoryRoot(path: string): Promise<string> {
  return (await git(path, ['rev-parse', '--show-toplevel'])).trim()
}

// The local branches of a repository, sorted, and the one its checkout is on.
export async function listBranches(
  repoDir: string,
): Promise<{ branches: string[]; current: string | null }> {
  const output = await git(repoDir, ['for-each-ref', '--format=%(refname:short)', 'refs/heads'])
  const branches = output
    .split('\n')
    .map((line) => line.trim())
    .filter((line) => line !== '')
    .sort((a, b) => a.localeCompare(b))
  return { branches, current: await currentBranch(repoDir) }
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
