import { mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import type { Task } from '@natamux/core'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { EventBus } from './event-bus.js'
import { GitHubError, type RunGh } from './git.js'
import { isWatched, PullRequestWatcher, repoFolderOr } from './pull-request-watcher.js'
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

describe('PullRequestWatcher', () => {
  it('reads the status of every watched pull request', async () => {
    const first = taskWithPr(1)
    const second = taskWithPr(2)
    tasks.createTask({ title: 'No pull request' })
    const asked: string[] = []
    const gh: RunGh = async (_cwd, args) => {
      asked.push(args[2] ?? '')
      return JSON.stringify({ state: 'OPEN', isDraft: false, statusCheckRollup: [] })
    }

    await new PullRequestWatcher({ tasks, cwdOf: () => '/repo', gh }).checkAll()

    expect(asked.sort()).toEqual([first.pr, second.pr])
    expect(tasks.getTask(first.id).prStatus).toEqual({ state: 'open', checks: 'none' })
  })

  it('runs gh in the folder of the repository of each task', async () => {
    taskWithPr(1)
    taskWithPr(2, 'web')
    const folders: string[] = []
    const gh: RunGh = async (cwd) => {
      folders.push(cwd)
      return JSON.stringify({ state: 'OPEN', isDraft: false, statusCheckRollup: [] })
    }
    const cwdOf = (repo: string | null) => (repo === null ? '/default' : `/${repo}`)

    await new PullRequestWatcher({ tasks, cwdOf, gh }).checkAll()

    expect(folders.sort()).toEqual(['/default', '/web'])
  })

  it('runs gh in each project folder, and in the data folder for a removed project', async () => {
    const watched = [
      taskWithPr(1, 'default'),
      taskWithPr(2, 'p2'),
      taskWithPr(3),
      taskWithPr(4, 'gone'),
    ]
    const folderOf: Record<string, string> = { default: '/a', p2: '/b' }
    const repos = {
      pathOf: (repo: string | null) => {
        const folder = folderOf[repo ?? 'default']
        if (folder === undefined) {
          throw new RepoNotFoundError(repo ?? 'default')
        }
        return folder
      },
    }
    const asked = new Map<string, string>()
    const gh: RunGh = async (cwd, args) => {
      asked.set(args[2] ?? '', cwd)
      return JSON.stringify({ state: 'MERGED', isDraft: false, statusCheckRollup: [] })
    }

    await new PullRequestWatcher({ tasks, cwdOf: repoFolderOr(repos, '/data'), gh }).checkAll()

    expect(watched.map((task) => asked.get(task.pr ?? ''))).toEqual(['/a', '/b', '/a', '/data'])
    for (const task of watched) {
      expect(tasks.getTask(task.id).prStatus?.state).toBe('merged')
    }
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
})
