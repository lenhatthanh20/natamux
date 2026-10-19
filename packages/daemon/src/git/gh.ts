import type { PullRequestStatus } from '@natamux/core'
import { errorText, execFileAsync, MAX_OUTPUT_BYTES } from './exec.js'

export class GitHubError extends Error {
  // What gh printed before it failed. A GraphQL answer with errors still has its data there.
  readonly output: string

  constructor(detail: string, output = '') {
    super(`GitHub CLI failed: ${detail}`)
    this.name = 'GitHubError'
    this.output = output
  }
}

export type RunGh = (cwd: string, args: readonly string[]) => Promise<string>

// A gh that hangs, for example after the machine sleeps, must not block every later PR check.
const GH_TIMEOUT_MS = 60_000

// Runs the GitHub CLI without prompts. Its errors become GitHubError.
export const runGh: RunGh = async (cwd, args) => {
  try {
    const { stdout } = await execFileAsync('gh', [...args], {
      cwd,
      maxBuffer: MAX_OUTPUT_BYTES,
      env: { ...process.env, GH_PROMPT_DISABLED: '1', GH_NO_UPDATE_NOTIFIER: '1', NO_COLOR: '1' },
      timeout: GH_TIMEOUT_MS,
    })
    return stdout
  } catch (error) {
    throw ghError(error)
  }
}

// The GitHubError for an error of execFile('gh', ...).
export function ghError(error: unknown): GitHubError {
  if (typeof error === 'object' && error !== null) {
    if ('code' in error && error.code === 'ENOENT') {
      return new GitHubError('gh is not installed or not on the PATH')
    }
    // The default text of a killed gh is only "Command failed".
    if ('killed' in error && error.killed === true) {
      return new GitHubError(`gh did not finish in ${GH_TIMEOUT_MS / 1000} seconds`)
    }
  }
  const output =
    typeof error === 'object' && error !== null && 'stdout' in error ? String(error.stdout) : ''
  return new GitHubError(errorText(error), output)
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

function pullRequestStatusOf(
  data: { state?: string; isDraft?: boolean },
  checks: readonly CheckItem[],
): PullRequestStatus {
  const state =
    data.state === 'MERGED'
      ? 'merged'
      : data.state === 'CLOSED'
        ? 'closed'
        : data.isDraft
          ? 'draft'
          : 'open'
  return { state, checks: summarizeChecks(checks) }
}

// Reads the output of `gh pr view --json state,isDraft,statusCheckRollup`.
export function parsePullRequestView(output: string): PullRequestStatus {
  const data = JSON.parse(output) as {
    state?: string
    isDraft?: boolean
    statusCheckRollup?: CheckItem[] | null
  }
  return pullRequestStatusOf(data, data.statusCheckRollup ?? [])
}

export type PullRequestRef = { host: string; owner: string; name: string; number: number }

const PULL_REQUEST_PARTS = /^https:\/\/([^/\s]+)\/([^/\s]+)\/([^/\s]+)\/pull\/(\d+)/

// The repository and number of a pull request address, or undefined.
export function parsePullRequestRef(url: string): PullRequestRef | undefined {
  const match = PULL_REQUEST_PARTS.exec(url)
  if (!match) {
    return undefined
  }
  const [, host = '', owner = '', name = '', number = ''] = match
  return { host, owner, name, number: Number(number) }
}

// The same as `gh pr view --json state,isDraft,statusCheckRollup` reads.
const PULL_REQUEST_FIELDS = [
  'state isDraft commits(last: 1) { nodes { commit { statusCheckRollup {',
  'contexts(first: 100) { nodes { ... on CheckRun { status conclusion }',
  '... on StatusContext { state } } } } } } }',
].join(' ')

type PullRequestNode = {
  state?: string
  isDraft?: boolean
  commits?: {
    nodes?: { commit?: { statusCheckRollup?: { contexts?: { nodes?: CheckItem[] } } | null } }[]
  }
}

// Reads the answer of readPullRequestStatuses. A number GitHub did not find is left out.
export function parsePullRequestBatch(
  output: string,
  numbers: readonly number[],
): Map<number, PullRequestStatus> {
  const data = JSON.parse(output) as {
    data?: { repository?: Record<string, PullRequestNode | null> | null }
  }
  const repository = data.data?.repository
  if (!repository) {
    throw new Error('no repository in the answer')
  }
  const statuses = new Map<number, PullRequestStatus>()
  for (const number of numbers) {
    const node = repository[`pr${number}`]
    if (node) {
      const rollup = node.commits?.nodes?.[0]?.commit?.statusCheckRollup
      statuses.set(number, pullRequestStatusOf(node, rollup?.contexts?.nodes ?? []))
    }
  }
  return statuses
}

// The pull requests of one repository with one gh call, instead of one `gh pr view` each.
export async function readPullRequestStatuses(
  cwd: string,
  repo: Omit<PullRequestRef, 'number'>,
  numbers: readonly number[],
  gh: RunGh = runGh,
): Promise<Map<number, PullRequestStatus>> {
  const fields = numbers.map(
    (number) => `pr${number}: pullRequest(number: ${number}) { ${PULL_REQUEST_FIELDS} }`,
  )
  const query = [
    'query($owner: String!, $name: String!) { repository(owner: $owner, name: $name) {',
    ...fields,
    '} }',
  ].join(' ')
  const host = repo.host === 'github.com' ? [] : ['--hostname', repo.host]
  const names = ['-f', `owner=${repo.owner}`, '-f', `name=${repo.name}`]
  let output: string
  try {
    output = await gh(cwd, ['api', 'graphql', ...host, '-f', `query=${query}`, ...names])
  } catch (error) {
    // gh fails when one number is not found, and still prints the others.
    if (!(error instanceof GitHubError) || error.output === '') {
      throw error
    }
    try {
      return parsePullRequestBatch(error.output, numbers)
    } catch {
      throw error
    }
  }
  try {
    return parsePullRequestBatch(output, numbers)
  } catch {
    throw new GitHubError(`could not read the pull requests: ${output.slice(0, 200)}`)
  }
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

// The address of the newest pull request from `branch`, when it is open or merged. A closed one
// is left out: the work it had was dropped.
export async function findPullRequest(
  cwd: string,
  branch: string,
  gh: RunGh = runGh,
): Promise<string | undefined> {
  const args = ['--head', branch, '--state', 'all', '--json', 'url,state', '--limit', '1']
  const output = await gh(cwd, ['pr', 'list', ...args])
  let found: { url?: unknown; state?: unknown }[]
  try {
    found = JSON.parse(output)
  } catch {
    throw new GitHubError(`could not read the pull requests: ${output.slice(0, 200)}`)
  }
  const pr = found[0]
  return typeof pr?.url === 'string' && pr.state !== 'CLOSED' ? pr.url : undefined
}

// How many of the newest pull requests findPullRequests reads. shortcut: a branch whose pull
// request is older is not found here; findPullRequest (on an agent's stop) still finds it.
const LISTED_PULL_REQUESTS = 100

// The address of the newest pull request of each branch, by branch, from one gh call for the
// whole repository. Left out as in findPullRequest: a branch whose newest one is closed.
export async function findPullRequests(
  cwd: string,
  gh: RunGh = runGh,
): Promise<Map<string, string>> {
  const args = ['--state', 'all', '--json', 'url,state,headRefName']
  const output = await gh(cwd, ['pr', 'list', ...args, '--limit', String(LISTED_PULL_REQUESTS)])
  let found: unknown
  try {
    found = JSON.parse(output)
  } catch {
    found = undefined
  }
  if (!Array.isArray(found)) {
    throw new GitHubError(`could not read the pull requests: ${output.slice(0, 200)}`)
  }
  const byBranch = new Map<string, string>()
  const seen = new Set<string>()
  // Newest first, so the first one of a branch is its newest.
  for (const { url, state, headRefName } of found as Record<string, unknown>[]) {
    if (typeof headRefName !== 'string' || seen.has(headRefName)) {
      continue
    }
    seen.add(headRefName)
    if (typeof url === 'string' && state !== 'CLOSED') {
      byBranch.set(headRefName, url)
    }
  }
  return byBranch
}
