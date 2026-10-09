import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import {
  DEFAULT_PROJECT_SETTINGS,
  LEGACY_PIPELINE,
  type ProjectSettings,
  type Task,
} from '@helmsman/core'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { EventBus } from './event-bus.js'
import { closingLine, IssueLinkError, IssueLinker, issueUrl } from './issue-linker.js'
import { archiveDir } from './stage-files.js'
import { TaskIndex } from './task-index.js'
import { TaskService } from './task-service.js'

let dataDir: string
let index: TaskIndex
let tasks: TaskService
let linker: IssueLinker
let settings: ProjectSettings
let ghCalls: string[][]

beforeEach(() => {
  dataDir = mkdtempSync(join(tmpdir(), 'helmsman-issues-'))
  index = new TaskIndex(':memory:')
  settings = DEFAULT_PROJECT_SETTINGS
  const bus = new EventBus()
  tasks = new TaskService({ dataDir, index, bus, settingsOf: () => settings })
  ghCalls = []
  linker = new IssueLinker({
    tasks,
    bus,
    dataDir,
    repoPath: () => '/repo',
    gh: async (_cwd, args) => {
      ghCalls.push([...args])
      return 'https://github.com/o/r/issues/12\n'
    },
  })
  linker.start()
})

afterEach(() => {
  linker.stop()
  index.close()
  rmSync(dataDir, { recursive: true, force: true })
})

function atSpecReview(): Task {
  const task = tasks.createTask({ title: 'Export tasks' })
  tasks.applyEvent(task.id, {
    source: 'human',
    type: 'state.overridden',
    payload: { to: 'review', stage: 'spec' },
  })
  mkdirSync(archiveDir(dataDir, task.id), { recursive: true })
  writeFileSync(join(archiveDir(dataDir, task.id), 'spec.md'), '# Goal\nExport tasks as CSV.\n')
  return tasks.getTask(task.id)
}

describe('IssueLinker', () => {
  it('opens an issue with the spec when the spec is approved', async () => {
    const task = atSpecReview()
    tasks.applyEvent(task.id, {
      source: 'human',
      type: 'stage.approved',
      payload: { stage: 'spec' },
    })
    await linker.settle()

    expect(ghCalls[0]?.slice(0, 4)).toEqual(['issue', 'create', '--title', 'Export tasks'])
    expect(ghCalls[0]?.[5]).toContain('Export tasks as CSV.')
    expect(tasks.getTask(task.id).issue).toEqual({
      provider: 'github',
      key: '#12',
      url: 'https://github.com/o/r/issues/12',
    })
  })

  it('opens no issue when the project does not want one, or the task has one', async () => {
    settings = {
      ...DEFAULT_PROJECT_SETTINGS,
      tracker: { ...DEFAULT_PROJECT_SETTINGS.tracker, createIssue: 'manual' },
    }
    const task = atSpecReview()
    tasks.applyEvent(task.id, {
      source: 'human',
      type: 'stage.approved',
      payload: { stage: 'spec' },
    })
    await linker.settle()
    expect(ghCalls).toEqual([])

    await linker.create(task.id)
    await expect(linker.create(task.id)).rejects.toThrow(IssueLinkError)
  })

  it('ends the pull request body with the closing line', async () => {
    const task = tasks.createTask({ title: 'Export', description: 'Adds a CSV export.' })
    tasks.linkIssue(task.id, {
      provider: 'github',
      key: '#3',
      url: 'https://github.com/o/r/issues/3',
    })

    expect(await linker.pullRequestBody(tasks.getTask(task.id))).toBe(
      'Adds a CSV export.\n\nCloses #3',
    )
  })

  it('opens the issue with the pull request in a pipeline without a spec stage', async () => {
    settings = { ...DEFAULT_PROJECT_SETTINGS, stages: LEGACY_PIPELINE }
    const task = tasks.createTask({ title: 'Export', description: 'Adds a CSV export.' })

    expect(await linker.pullRequestBody(task)).toBe('Adds a CSV export.\n\nCloses #12')
  })
})

describe('helpers', () => {
  it('finds the issue address and writes the closing line', () => {
    expect(issueUrl('Creating issue\nhttps://github.com/o/r/issues/9\n')).toBe(
      'https://github.com/o/r/issues/9',
    )
    expect(
      closingLine({ provider: 'github', key: '#9', url: 'https://github.com/o/r/issues/9' }),
    ).toBe('Closes #9')
    expect(
      closingLine({ provider: 'jira', key: 'ABC-1', url: 'https://jira.example.com/ABC-1' }),
    ).toBe('Tracked in ABC-1: https://jira.example.com/ABC-1')
  })
})
