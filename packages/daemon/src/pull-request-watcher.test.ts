import { mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import type { Task } from '@natamux/core'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { EventBus } from './event-bus.js'
import { GitHubError, type RunGh } from './git.js'
import {
  isWatched,
  PullRequestWatcher,
  repoFolderOr,
  SETTLED_INTERVAL_MS,
  WATCH_INTERVAL_MS,
} from './pull-request-watcher.js'
import { RepoNotFoundError } from './repo-service.js'
import { TaskIndex } from './task-index.js'
import { TaskService } from './task-service.js'

let dataDir: string
let index: TaskIndex
let tasks: TaskService

beforeEach(() => {
  dataDir = mkdtempSync(join(tmpdir(), 'natamux-watch-'))
  index = new TaskIndex(':memory:')
  tasks = new TaskService({ dataDir, index, bus: new EventBus() })
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
