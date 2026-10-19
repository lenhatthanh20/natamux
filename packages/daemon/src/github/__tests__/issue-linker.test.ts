import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import {
  DEFAULT_PROJECT_SETTINGS,
  LEGACY_PIPELINE,
  type ProjectSettings,
  type Task,
} from '@natamux/core'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { EventBus } from '../../shared/event-bus.js'
import { archiveDir } from '../../tasks/task-files.js'
import { TaskIndex } from '../../tasks/task-index.js'
import { TaskService } from '../../tasks/task-service.js'
import { closingLine, IssueLinkError, IssueLinker, issueUrl } from '../issue-linker.js'

let dataDir: string
let index: TaskIndex
let tasks: TaskService
let linker: IssueLinker
let settings: ProjectSettings
let ghCalls: string[][]

beforeEach(() => {
  dataDir = mkdtempSync(join(tmpdir(), 'natamux-issues-'))
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

// The project asks for an issue on its own; by default it does not.
function opensIssues(createIssue: ProjectSettings['tracker']['createIssue']): ProjectSettings {
  return {
    ...DEFAULT_PROJECT_SETTINGS,
    tracker: { ...DEFAULT_PROJECT_SETTINGS.tracker, createIssue },
  }
}

describe('IssueLinker', () => {
  it('opens no issue by default, on spec approval or with the pull request', async () => {
    const task = atSpecReview()
    tasks.applyEvent(task.id, {
      source: 'human',
      type: 'stage.approved',
      payload: { stage: 'spec' },
    })
    await linker.settle()
    settings = { ...DEFAULT_PROJECT_SETTINGS, stages: LEGACY_PIPELINE }
    const other = tasks.createTask({ title: 'Export', description: 'Adds a CSV export.' })

    expect(await linker.pullRequestBody(other)).toBe('Adds a CSV export.')
    expect(ghCalls).toEqual([])
    expect(tasks.getTask(task.id).issue).toBeNull()
  })

  it('opens an issue with the spec when the spec is approved', async () => {
    settings = opensIssues('on-spec-approved')
    const task = atSpecReview()
    tasks.applyEvent(task.id, {
      source: 'human',
      type: 'stage.approved',
      payload: { stage: 'spec' },
    })
    await linker.settle()

    expect(ghCalls[0]?.slice(0, 4)).toEqual(['issue', 'create', '--title', 'Export tasks'])
    expect(ghCalls[0]?.[5]).toContain('Export tasks as CSV.')
    expect(ghCalls[0]?.[5]).toContain(`Made by Natamux for ${task.id}.`)
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

  it('opens one issue for two requests at the same time', async () => {
    const task = tasks.createTask({ title: 'Export' })

    const [first, second] = await Promise.all([linker.create(task.id), linker.create(task.id)])

    expect(ghCalls).toHaveLength(1)
    expect(first.issue?.key).toBe('#12')
    expect(second).toEqual(first)
  })

  it('does not link a new issue over one linked while it was opened', async () => {
    const task = tasks.createTask({ title: 'Export' })
    const opening = linker.create(task.id)
    tasks.linkIssue(task.id, {
      provider: 'github',
      key: '#3',
      url: 'https://github.com/o/r/issues/3',
    })

    await expect(opening).rejects.toThrow(/linked to #3/)
    expect(tasks.getTask(task.id).issue?.key).toBe('#3')
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
    settings = { ...opensIssues('on-spec-approved'), stages: LEGACY_PIPELINE }
    const task = tasks.createTask({ title: 'Export', description: 'Adds a CSV export.' })

    expect(await linker.pullRequestBody(task)).toBe('Adds a CSV export.\n\nCloses #12')
  })

  it('opens the issue with the pull request, not on spec approval, for on-pr', async () => {
    settings = opensIssues('on-pr')
    const task = atSpecReview()
    tasks.applyEvent(task.id, {
      source: 'human',
      type: 'stage.approved',
      payload: { stage: 'spec' },
    })
    await linker.settle()
    expect(ghCalls).toEqual([])

    tasks.updateTask(task.id, { description: 'Adds a CSV export.' })
    expect(await linker.pullRequestBody(tasks.getTask(task.id))).toBe(
      'Adds a CSV export.\n\nCloses #12',
    )
    expect(ghCalls).toHaveLength(1)
    expect(await linker.pullRequestBody(tasks.getTask(task.id))).toBe(
      'Adds a CSV export.\n\nCloses #12',
    )
    expect(ghCalls).toHaveLength(1)
  })

  it('opens no issue for never, on spec approval or with the pull request', async () => {
    settings = { ...opensIssues('never'), stages: LEGACY_PIPELINE }
    const task = tasks.createTask({ title: 'Export', description: 'Adds a CSV export.' })

    expect(await linker.pullRequestBody(task)).toBe('Adds a CSV export.')
    expect(ghCalls).toEqual([])
  })

  it('writes the task notes in the body when there is no spec', async () => {
    const task = tasks.createTask({ title: 'Export', description: 'Adds a CSV export.' })

    await linker.create(task.id)

    expect(ghCalls[0]?.[5]).toBe(`Adds a CSV export.\n\nMade by Natamux for ${task.id}.`)
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
