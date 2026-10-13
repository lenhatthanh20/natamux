import { mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { EventBus } from './event-bus.js'
import { GitHubError, type RunGh } from './git.js'
import { IssueImporter, parseIssueList } from './issue-importer.js'
import { TaskIndex } from './task-index.js'
import { TaskEditError, TaskService } from './task-service.js'

const ISSUES = [
  {
    number: 3,
    title: 'Crash on start',
    body: 'Steps:\n1. Start',
    url: 'https://github.com/o/r/issues/3',
    labels: [{ name: 'bug' }],
  },
  { number: 4, title: '', body: '', url: 'https://github.com/o/r/issues/4', labels: [] },
]

let dataDir: string
let index: TaskIndex
let tasks: TaskService
let calls: (readonly string[])[]
let importer: IssueImporter

beforeEach(() => {
  dataDir = mkdtempSync(join(tmpdir(), 'natamux-issues-'))
  index = new TaskIndex(':memory:')
  tasks = new TaskService({ dataDir, index, bus: new EventBus() })
  calls = []
  const gh: RunGh = async (_cwd, args) => {
    calls.push(args)
    return JSON.stringify(ISSUES)
  }
  importer = new IssueImporter({ tasks, repoPath: () => '/repo', gh })
})

afterEach(() => {
  index.close()
  rmSync(dataDir, { recursive: true, force: true })
})

describe('parseIssueList', () => {
  it('keeps the fields it needs and skips items without a number or an address', () => {
    expect(parseIssueList(JSON.stringify([...ISSUES, { title: 'broken' }]))).toEqual([
      {
        number: 3,
        title: 'Crash on start',
        body: 'Steps:\n1. Start',
        url: 'https://github.com/o/r/issues/3',
        labels: ['bug'],
      },
      { number: 4, title: '', body: '', url: 'https://github.com/o/r/issues/4', labels: [] },
    ])
  })

  it('fails with a GitHubError on an answer that is not JSON', () => {
    expect(() => parseIssueList('nope')).toThrow(GitHubError)
  })
})

describe('IssueImporter', () => {
  it('stops at an issue whose title names an id that is used', async () => {
    tasks.createTask({ title: 'ABC-9: Existing' })
    const issue = (number: number, title: string) => ({
      number,
      title,
      body: '',
      url: `https://github.com/o/r/issues/${number}`,
      labels: [],
    })
    const issues = [issue(5, 'One'), issue(6, 'ABC-9: Two'), issue(7, 'Three')]
    const clashing = new IssueImporter({
      tasks,
      repoPath: () => '/repo',
      gh: async () => JSON.stringify(issues),
    })

    await expect(clashing.import([5, 6, 7])).rejects.toThrow(TaskEditError)
    expect(
      tasks
        .listTasks()
        .map((task) => task.title)
        .sort(),
    ).toEqual(['Existing', 'One'])
  })

  it('lists the open issues with gh', async () => {
    const issues = await importer.list()

    expect(issues.map((issue) => [issue.number, issue.taskId])).toEqual([
      [3, null],
      [4, null],
    ])
    expect(calls[0]).toEqual([
      'issue',
      'list',
      '--state',
      'open',
      '--limit',
      '100',
      '--json',
      'number,title,body,url,labels',
    ])
  })

  it('makes a draft task from an issue, once', async () => {
    const [task] = await importer.import([3])

    expect(task).toMatchObject({
      title: 'Crash on start',
      description: 'Steps:\n1. Start\n\nImported from https://github.com/o/r/issues/3',
      labels: ['bug'],
      state: 'draft',
    })
    expect(await importer.import([3])).toEqual([])
    expect((await importer.list()).find((issue) => issue.number === 3)?.taskId).toBe(task?.id)
  })

  it('names an issue without a title, and skips numbers that are not open issues', async () => {
    const created = await importer.import([4, 99])

    expect(created).toHaveLength(1)
    expect(created[0]).toMatchObject({
      title: 'Issue #4',
      description: 'Imported from https://github.com/o/r/issues/4',
    })
  })
})
