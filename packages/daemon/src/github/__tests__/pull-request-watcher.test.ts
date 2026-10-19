import { mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import type { Task, TaskEvent } from '@natamux/core'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { GitHubError, type RunGh } from '../../git/gh.js'
import { RepoNotFoundError, repoFolderOr } from '../../projects/repo-service.js'
import { EventBus } from '../../shared/event-bus.js'
import { TaskIndex } from '../../tasks/task-index.js'
import { TaskService } from '../../tasks/task-service.js'
import {
  isCandidate,
  isWatched,
  PullRequestWatcher,
  SETTLED_INTERVAL_MS,
  WATCH_INTERVAL_MS,
} from '../pull-request-watcher.js'

let dataDir: string
let index: TaskIndex
let tasks: TaskService
// The bus of the task service.
let taskBus: EventBus

beforeEach(() => {
  dataDir = mkdtempSync(join(tmpdir(), 'natamux-watch-'))
  index = new TaskIndex(':memory:')
  taskBus = new EventBus()
  tasks = new TaskService({ dataDir, index, bus: taskBus })
})

afterEach(() => {
  index.close()
  rmSync(dataDir, { recursive: true, force: true })
})

function taskWithPr(number: number, repo?: string): Task {
  const task = tasks.createTask({
    title: `Task ${number}`,
    ...(repo === undefined ? {} : { repo }),
  })
  return tasks.recordGit(task.id, 'pr.opened', { url: `https://github.com/o/r/pull/${number}` })
}

describe('isWatched', () => {
  it('watches a task with a pull request that is not merged or closed', () => {
    const task = taskWithPr(1)
    expect(isWatched(task)).toBe(true)
    expect(isWatched({ ...task, prStatus: { state: 'draft', checks: 'none' } })).toBe(true)
    expect(isWatched({ ...task, prStatus: { state: 'merged', checks: 'none' } })).toBe(false)
    expect(isWatched({ ...task, prStatus: { state: 'closed', checks: 'none' } })).toBe(false)
    expect(isWatched({ ...task, state: 'cancelled' })).toBe(false)
    expect(isWatched({ ...task, pr: null })).toBe(false)
  })
})

// Answers like `gh api graphql`, with each pull request asked for in its given state. A number
// without a state is not found.
function graphql(states: Record<number, string>, calls: GhCall[] = []): RunGh {
  return async (cwd, args) => {
    calls.push({ cwd, args })
    const query = args.find((arg) => arg.startsWith('query=')) ?? ''
    const matches = query.matchAll(/pullRequest\(number: (\d+)\)/g)
    const numbers = [...matches].map(([, number]) => Number(number))
    const repository = Object.fromEntries(
      numbers.map((number) => {
        const state = states[number]
        const node = state === undefined ? null : { state, isDraft: false, commits: { nodes: [] } }
        return [`pr${number}`, node]
      }),
    )
    return JSON.stringify({ data: { repository } })
  }
}

type GhCall = { cwd: string; args: readonly string[] }

// The values that follow `flag` in the arguments of a gh call.
function valuesOf(call: GhCall | undefined, flag: string): string[] {
  const args = call?.args ?? []
  return args.filter((_, at) => args[at - 1] === flag)
}

function busWithClient(): EventBus {
  const bus = new EventBus()
  bus.subscribeClient(() => undefined)
  return bus
}

describe('PullRequestWatcher', () => {
  afterEach(() => {
    vi.useRealTimers()
  })

  it('reads the status of every watched pull request', async () => {
    const first = taskWithPr(1)
    const second = taskWithPr(2)
    tasks.createTask({ title: 'No pull request' })
    const calls: GhCall[] = []
    const gh = graphql({ 1: 'OPEN', 2: 'MERGED' }, calls)

    await new PullRequestWatcher({ tasks, cwdOf: () => '/repo', gh }).checkAll()

    expect(calls).toHaveLength(1)
    expect(tasks.getTask(first.id).prStatus).toEqual({ state: 'open', checks: 'none' })
    expect(tasks.getTask(second.id).prStatus).toEqual({ state: 'merged', checks: 'none' })
  })

  it('asks gh once per GitHub repository, in the folder of one of its tasks', async () => {
    taskWithPr(1)
    taskWithPr(2)
    const web = tasks.createTask({ title: 'Web', repo: 'web' })
    tasks.recordGit(web.id, 'pr.opened', { url: 'https://github.com/o/web/pull/3' })
    const calls: GhCall[] = []
    const cwdOf = (repo: string | null) => (repo === null ? '/default' : `/${repo}`)

    await new PullRequestWatcher({ tasks, cwdOf, gh: graphql({}, calls) }).checkAll()

    const byName = new Map(calls.map((call) => [valuesOf(call, '-f').at(-1), call]))
    expect(calls).toHaveLength(2)
    expect(byName.get('name=r')?.cwd).toBe('/default')
    expect(byName.get('name=web')?.cwd).toBe('/web')
    expect(valuesOf(byName.get('name=r'), '-f')).toContain('owner=o')
  })

  it('runs gh in the data folder for a removed project', async () => {
    const task = tasks.createTask({ title: 'Gone', repo: 'gone' })
    tasks.recordGit(task.id, 'pr.opened', { url: 'https://github.com/o/r/pull/4' })
    const repos = {
      pathOf: (repo: string | null) => {
        throw new RepoNotFoundError(repo ?? 'default')
      },
    }
    const calls: GhCall[] = []
    const gh = graphql({ 4: 'MERGED' }, calls)

    await new PullRequestWatcher({ tasks, cwdOf: repoFolderOr(repos, '/data'), gh }).checkAll()

    expect(calls.map((call) => call.cwd)).toEqual(['/data'])
    expect(tasks.getTask(task.id).prStatus?.state).toBe('merged')
  })

  it('leaves a pull request that GitHub did not find, and reads the others', async () => {
    const found = taskWithPr(1)
    const missing = taskWithPr(2)

    const gh = graphql({ 1: 'OPEN' })

    await new PullRequestWatcher({ tasks, cwdOf: () => '/repo', gh }).checkAll()

    expect(tasks.getTask(found.id).prStatus?.state).toBe('open')
    expect(tasks.getTask(missing.id).prStatus).toBeNull()
  })

  it('goes on after a failure and reports the same failure once', async () => {
    taskWithPr(1)
    taskWithPr(2)
    const logged: string[] = []
    const gh: RunGh = async () => {
      throw new GitHubError('gh is not installed or not on the PATH')
    }
    const watcher = new PullRequestWatcher({
      tasks,
      cwdOf: () => '/repo',
      gh,
      log: (message) => logged.push(message),
    })

    await watcher.checkAll()
    await watcher.checkAll()

    expect(logged).toEqual([
      'Pull request check: GitHub CLI failed: gh is not installed or not on the PATH',
    ])
  })

  it('asks nothing while no dashboard listens, and once when the first one comes', async () => {
    vi.useFakeTimers()
    taskWithPr(1)
    const calls: GhCall[] = []
    const bus = new EventBus()
    const watcher = new PullRequestWatcher({
      tasks,
      cwdOf: () => '/repo',
      gh: graphql({ 1: 'OPEN' }, calls),
      bus,
    })
    watcher.start()
    await vi.advanceTimersByTimeAsync(WATCH_INTERVAL_MS * 3)
    expect(calls).toHaveLength(0)

    const leave = bus.subscribeClient(() => undefined)
    watcher.wake()
    await vi.advanceTimersByTimeAsync(0)
    expect(calls).toHaveLength(1)

    leave()
    await vi.advanceTimersByTimeAsync(SETTLED_INTERVAL_MS * 3)
    expect(calls).toHaveLength(1)
    watcher.stop()
  })

  it('asks every 5 minutes once all checks settled, sooner for a new pull request', async () => {
    vi.useFakeTimers()
    taskWithPr(1)
    const calls: GhCall[] = []
    const watcher = new PullRequestWatcher({
      tasks,
      cwdOf: () => '/repo',
      gh: graphql({ 1: 'OPEN', 2: 'OPEN' }, calls),
      bus: busWithClient(),
    })

    // A dashboard listens already: the first check is at once.
    watcher.start()
    await vi.advanceTimersByTimeAsync(0)
    expect(calls).toHaveLength(1)
    await vi.advanceTimersByTimeAsync(SETTLED_INTERVAL_MS - WATCH_INTERVAL_MS)
    expect(calls).toHaveLength(1)
    await vi.advanceTimersByTimeAsync(WATCH_INTERVAL_MS)
    expect(calls).toHaveLength(2)

    taskWithPr(2)
    await vi.advanceTimersByTimeAsync(WATCH_INTERVAL_MS)
    expect(calls).toHaveLength(3)
    watcher.stop()
  })

  it('slows down while gh keeps failing', async () => {
    vi.useFakeTimers()
    taskWithPr(1)
    let calls = 0
    const gh: RunGh = async () => {
      calls += 1
      throw new GitHubError('HTTP 502')
    }
    const watcher = new PullRequestWatcher({
      tasks,
      cwdOf: () => '/repo',
      gh,
      bus: busWithClient(),
      log: () => undefined,
    })

    watcher.start()
    await vi.advanceTimersByTimeAsync(WATCH_INTERVAL_MS * 2)
    expect(calls).toBe(2)
    await vi.advanceTimersByTimeAsync(WATCH_INTERVAL_MS * 3)
    expect(calls).toBe(2)
    await vi.advanceTimersByTimeAsync(WATCH_INTERVAL_MS)
    expect(calls).toBe(3)
    watcher.stop()
  })

  it('asks every minute again once a settled pull request has a check that runs', async () => {
    vi.useFakeTimers()
    const task = taskWithPr(1)
    let checks: object[] = []
    let calls = 0
    const gh: RunGh = async () => {
      calls += 1
      const commit = { statusCheckRollup: { contexts: { nodes: checks } } }
      const node = { state: 'OPEN', isDraft: false, commits: { nodes: [{ commit }] } }
      return JSON.stringify({ data: { repository: { pr1: node } } })
    }
    const watcher = new PullRequestWatcher({
      tasks,
      cwdOf: () => '/repo',
      gh,
      bus: busWithClient(),
    })
    watcher.start()
    await vi.advanceTimersByTimeAsync(0)
    expect(calls).toBe(1)

    // A new commit was pushed: GitHub runs its checks.
    checks = [{ status: 'IN_PROGRESS', conclusion: null }]
    await vi.advanceTimersByTimeAsync(SETTLED_INTERVAL_MS)
    expect(calls).toBe(2)

    await vi.advanceTimersByTimeAsync(WATCH_INTERVAL_MS)
    expect(calls).toBe(3)
    expect(tasks.getTask(task.id).prStatus).toEqual({ state: 'open', checks: 'pending' })
    await vi.advanceTimersByTimeAsync(WATCH_INTERVAL_MS)
    expect(calls).toBe(4)
    watcher.stop()
  })
})

// A task with a workspace on `branch`, and no pull request yet.
function taskOnBranch(branch: string, repo?: string): Task {
  const task = tasks.createTask({ title: branch, ...(repo === undefined ? {} : { repo }) })
  return tasks.recordWorkspace(task.id, { workspace: `/work/${branch}`, branch })
}

type FoundPullRequest = { url: string; state: string }

// Answers `gh pr list --head` with the pull request of that branch, `gh pr list` without it with
// the pull request of each branch, and `gh pr view` and `gh api graphql` with the state of that
// pull request.
function github(prs: Record<string, FoundPullRequest>, calls: GhCall[] = []): RunGh {
  const byNumber = Object.fromEntries(
    Object.values(prs).map((pr) => [Number(pr.url.split('/').at(-1)), pr.state]),
  )
  const batch = graphql(byNumber)
  return async (cwd, args) => {
    calls.push({ cwd, args })
    if (args[0] === 'pr' && args[1] === 'list') {
      if (!args.includes('--head')) {
        const all = Object.entries(prs).map(([headRefName, pr]) => ({ ...pr, headRefName }))
        return JSON.stringify(all)
      }
      const pr = prs[valuesOf({ cwd, args }, '--head')[0] ?? '']
      return JSON.stringify(pr === undefined ? [] : [pr])
    }
    if (args[0] === 'pr' && args[1] === 'view') {
      const pr = Object.values(prs).find((candidate) => candidate.url === args[2])
      return JSON.stringify({ state: pr?.state ?? 'OPEN', isDraft: false, statusCheckRollup: [] })
    }
    return batch(cwd, args)
  }
}

const onOrigin = async () => true

describe('isCandidate', () => {
  it('looks for a pull request of an active task that has a branch and none yet', () => {
    const task = taskOnBranch('natamux/t-1')
    expect(isCandidate(task)).toBe(true)
    expect(isCandidate({ ...task, pr: 'https://github.com/o/r/pull/1' })).toBe(false)
    expect(isCandidate({ ...task, branch: null })).toBe(false)
    expect(isCandidate({ ...task, workspace: null })).toBe(false)
    expect(isCandidate({ ...task, state: 'cancelled' })).toBe(false)
    expect(isCandidate({ ...task, state: 'done' })).toBe(false)
  })
})

describe('PullRequestWatcher finds a pull request opened outside Natamux', () => {
  const url = 'https://github.com/o/r/pull/5'

  afterEach(() => {
    vi.useRealTimers()
  })

  it('links the pull request of the task branch and reads its status', async () => {
    const task = taskOnBranch('natamux/t-1')
    const calls: GhCall[] = []
    const gh = github({ 'natamux/t-1': { url, state: 'OPEN' } }, calls)
    const watcher = new PullRequestWatcher({
      tasks,
      cwdOf: () => '/repo',
      gh,
      hasRemoteBranch: onOrigin,
    })

    await watcher.checkAll()

    const linked = tasks.getTask(task.id)
    expect(linked.pr).toBe(url)
    expect(linked.prStatus).toEqual({ state: 'open', checks: 'none' })
    expect(calls[0]?.cwd).toBe('/work/natamux/t-1')
    const opened = tasks.listEvents(task.id).filter(({ event }) => event.type === 'pr.opened')
    expect(opened.map(({ event }) => event.payload)).toEqual([{ url, detected: 'true' }])
  })

  it('lists the pull requests once per project, in the workspace of one of its tasks', async () => {
    const first = taskOnBranch('natamux/t-1')
    const second = taskOnBranch('natamux/t-2')
    taskOnBranch('natamux/t-3')
    const web = taskOnBranch('natamux/t-4', 'web')
    const calls: GhCall[] = []
    const gh = github(
      {
        'natamux/t-1': { url: 'https://github.com/o/r/pull/1', state: 'OPEN' },
        'natamux/t-2': { url: 'https://github.com/o/r/pull/2', state: 'MERGED' },
        'natamux/t-4': { url: 'https://github.com/o/web/pull/4', state: 'OPEN' },
      },
      calls,
    )
    const watcher = new PullRequestWatcher({
      tasks,
      cwdOf: () => '/repo',
      gh,
      hasRemoteBranch: onOrigin,
    })

    await watcher.checkAll()

    const lists = calls.filter(({ args }) => args[0] === 'pr' && args[1] === 'list')
    expect(lists).toHaveLength(2)
    // Any task of the project will do.
    const [inDefault, inWeb] = lists.map(({ cwd }) => cwd).sort()
    expect(['/work/natamux/t-1', '/work/natamux/t-2', '/work/natamux/t-3']).toContain(inDefault)
    expect(inWeb).toBe('/work/natamux/t-4')
    expect(lists.every(({ args }) => !args.includes('--head'))).toBe(true)
    expect(tasks.getTask(first.id).pr).toBe('https://github.com/o/r/pull/1')
    expect(tasks.getTask(second.id).pr).toBe('https://github.com/o/r/pull/2')
    expect(tasks.getTask(web.id).pr).toBe('https://github.com/o/web/pull/4')
  })

  it('goes on with the other projects when gh fails for one', async () => {
    taskOnBranch('natamux/t-1')
    const web = taskOnBranch('natamux/t-2', 'web')
    const logged: string[] = []
    const answer = github({ 'natamux/t-2': { url, state: 'OPEN' } })
    const gh: RunGh = async (cwd, args) => {
      if (cwd === '/work/natamux/t-1') {
        throw new GitHubError('HTTP 502')
      }
      return answer(cwd, args)
    }
    const watcher = new PullRequestWatcher({
      tasks,
      cwdOf: () => '/repo',
      gh,
      hasRemoteBranch: onOrigin,
      log: (message) => logged.push(message),
    })

    await watcher.checkAll()

    expect(tasks.getTask(web.id).pr).toBe(url)
    expect(logged).toEqual(['Pull request check: GitHub CLI failed: HTTP 502'])
  })

  it('does not link a pull request twice when a check linked it meanwhile', async () => {
    const task = taskOnBranch('natamux/t-1')
    const answer = github({ 'natamux/t-1': { url, state: 'OPEN' } })
    const gh: RunGh = async (cwd, args) => {
      if (args[1] === 'list' && tasks.getTask(task.id).pr === null) {
        tasks.recordGit(task.id, 'pr.opened', { url })
      }
      return answer(cwd, args)
    }
    const watcher = new PullRequestWatcher({
      tasks,
      cwdOf: () => '/repo',
      gh,
      hasRemoteBranch: onOrigin,
    })

    await watcher.checkAll()

    const opened = tasks.listEvents(task.id).filter(({ event }) => event.type === 'pr.opened')
    expect(opened).toHaveLength(1)
  })

  it('asks GitHub nothing while origin does not have the branch', async () => {
    const task = taskOnBranch('natamux/t-1')
    const calls: GhCall[] = []
    const gh = github({ 'natamux/t-1': { url, state: 'OPEN' } }, calls)
    const watcher = new PullRequestWatcher({
      tasks,
      cwdOf: () => '/repo',
      gh,
      hasRemoteBranch: async () => false,
    })

    await watcher.checkAll()

    expect(calls).toHaveLength(0)
    expect(tasks.getTask(task.id).pr).toBeNull()
  })

  it('does not link a closed pull request', async () => {
    const task = taskOnBranch('natamux/t-1')
    const gh = github({ 'natamux/t-1': { url, state: 'CLOSED' } })
    const watcher = new PullRequestWatcher({
      tasks,
      cwdOf: () => '/repo',
      gh,
      hasRemoteBranch: onOrigin,
    })

    await watcher.checkAll()

    expect(tasks.getTask(task.id).pr).toBeNull()
  })

  it('looks when asked to check a task without a pull request', async () => {
    const task = taskOnBranch('natamux/t-1')
    const gh = github({ 'natamux/t-1': { url, state: 'MERGED' } })
    const watcher = new PullRequestWatcher({
      tasks,
      cwdOf: () => '/repo',
      gh,
      hasRemoteBranch: onOrigin,
    })

    const checked = await watcher.check(task.id)

    expect(checked.pr).toBe(url)
    expect(checked.prStatus?.state).toBe('merged')
  })

  it('looks when the agent of the task stops', async () => {
    const task = taskOnBranch('natamux/t-1')
    const gh = github({ 'natamux/t-1': { url, state: 'OPEN' } })
    const watcher = new PullRequestWatcher({
      tasks,
      cwdOf: () => '/repo',
      gh,
      hasRemoteBranch: onOrigin,
      bus: taskBus,
    })
    watcher.start()

    const event: TaskEvent = {
      taskId: task.id,
      source: 'system',
      type: 'agent.stopped',
      at: new Date().toISOString(),
      payload: {},
    }
    taskBus.publish({ kind: 'task.updated', task, event, accepted: true })

    await vi.waitFor(() => expect(tasks.getTask(task.id).prStatus?.state).toBe('open'))
    expect(tasks.getTask(task.id).pr).toBe(url)
    watcher.stop()
  })

  it('asks every minute while origin has the branch of a task with no pull request', async () => {
    vi.useFakeTimers()
    taskOnBranch('natamux/t-1')
    const calls: GhCall[] = []
    const watcher = new PullRequestWatcher({
      tasks,
      cwdOf: () => '/repo',
      gh: github({}, calls),
      hasRemoteBranch: onOrigin,
      bus: busWithClient(),
    })

    watcher.start()
    await vi.advanceTimersByTimeAsync(0)
    expect(calls).toHaveLength(1)
    await vi.advanceTimersByTimeAsync(WATCH_INTERVAL_MS)
    expect(calls).toHaveLength(2)
    await vi.advanceTimersByTimeAsync(WATCH_INTERVAL_MS)
    expect(calls).toHaveLength(3)
    watcher.stop()
  })

  it('asks every 5 minutes while origin does not have the branch', async () => {
    vi.useFakeTimers()
    taskOnBranch('natamux/t-1')
    let looks = 0
    const watcher = new PullRequestWatcher({
      tasks,
      cwdOf: () => '/repo',
      gh: github({}),
      hasRemoteBranch: async () => {
        looks += 1
        return false
      },
      bus: busWithClient(),
    })

    watcher.start()
    await vi.advanceTimersByTimeAsync(0)
    expect(looks).toBe(1)
    await vi.advanceTimersByTimeAsync(SETTLED_INTERVAL_MS - WATCH_INTERVAL_MS)
    expect(looks).toBe(1)
    await vi.advanceTimersByTimeAsync(WATCH_INTERVAL_MS)
    expect(looks).toBe(2)
    watcher.stop()
  })

  it('still slows down to 5 minutes while gh keeps failing for such a task', async () => {
    vi.useFakeTimers()
    taskOnBranch('natamux/t-1')
    let calls = 0
    const gh: RunGh = async () => {
      calls += 1
      throw new GitHubError('HTTP 502')
    }
    const watcher = new PullRequestWatcher({
      tasks,
      cwdOf: () => '/repo',
      gh,
      hasRemoteBranch: onOrigin,
      bus: busWithClient(),
      log: () => undefined,
    })

    watcher.start()
    await vi.advanceTimersByTimeAsync(WATCH_INTERVAL_MS * 2)
    expect(calls).toBe(2)
    await vi.advanceTimersByTimeAsync(WATCH_INTERVAL_MS * 3)
    expect(calls).toBe(2)
    await vi.advanceTimersByTimeAsync(WATCH_INTERVAL_MS)
    expect(calls).toBe(3)
    // From here on, never longer than 5 minutes.
    await vi.advanceTimersByTimeAsync(SETTLED_INTERVAL_MS)
    expect(calls).toBe(4)
    await vi.advanceTimersByTimeAsync(SETTLED_INTERVAL_MS)
    expect(calls).toBe(5)
    watcher.stop()
  })
})
