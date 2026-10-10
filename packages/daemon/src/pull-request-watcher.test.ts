import { mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import type { Task } from '@natamux/core'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { EventBus } from './event-bus.js'
import { GitHubError, type RunGh } from './git.js'
import { isWatched, PullRequestWatcher } from './pull-request-watcher.js'
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

function taskWithPr(number: number): Task {
  const task = tasks.createTask({ title: `Task ${number}` })
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

    await new PullRequestWatcher({ tasks, cwd: '/repo', gh }).checkAll()

    expect(asked.sort()).toEqual([first.pr, second.pr])
    expect(tasks.getTask(first.id).prStatus).toEqual({ state: 'open', checks: 'none' })
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
      cwd: '/repo',
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
