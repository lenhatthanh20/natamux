import { mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { type DaemonMessage, EventBus } from './event-bus.js'
import { readTaskFile } from './task-files.js'
import { TaskIndex } from './task-index.js'
import { TaskNotFoundError, TaskService } from './task-service.js'

let dataDir: string
let index: TaskIndex
let messages: DaemonMessage[]
let service: TaskService

function makeService(existingIndex: TaskIndex): TaskService {
  const bus = new EventBus()
  bus.subscribe((message) => messages.push(message))
  let minute = 0
  return new TaskService({
    dataDir,
    index: existingIndex,
    bus,
    now: () => new Date(Date.UTC(2026, 9, 8, 9, minute++, 0)),
  })
}

beforeEach(() => {
  dataDir = mkdtempSync(join(tmpdir(), 'helmsman-service-'))
  index = new TaskIndex(':memory:')
  messages = []
  service = makeService(index)
})

afterEach(() => {
  index.close()
  rmSync(dataDir, { recursive: true, force: true })
})

describe('createTask', () => {
  it('creates a draft task with the next id and stores it as a file', () => {
    const first = service.createTask({ title: 'First' })
    const second = service.createTask({ title: 'Second' })

    expect(first).toMatchObject({ id: 'T-0001', state: 'draft', priority: 'normal' })
    expect(second.id).toBe('T-0002')
    expect(readTaskFile(dataDir, 'T-0001')).toEqual(first)
  })

  it('records a task.created event and publishes the new task', () => {
    const task = service.createTask({ title: 'First' })

    expect(service.listEvents(task.id).map((entry) => entry.event.type)).toEqual(['task.created'])
    expect(messages).toEqual([{ kind: 'task.created', task }])
  })

  it('rejects an empty title', () => {
    expect(() => service.createTask({ title: '  ' })).toThrow()
  })
})

describe('applyEvent', () => {
  it('moves the task when the event fits, and stores, indexes and publishes the change', () => {
    const task = service.createTask({ title: 'First' })

    const result = service.applyEvent(task.id, { source: 'human', type: 'task.readied' })

    expect(result.accepted).toBe(true)
    expect(result.task.state).toBe('ready')
    expect(readTaskFile(dataDir, task.id)?.state).toBe('ready')
    expect(service.listTasks('ready').map((listed) => listed.id)).toEqual([task.id])
    expect(messages.at(-1)).toMatchObject({ kind: 'task.updated', accepted: true })
  })

  it('keeps the task when the event does not fit, but still records the event', () => {
    const task = service.createTask({ title: 'First' })

    const result = service.applyEvent(task.id, { source: 'agent', type: 'agent.finished' })

    expect(result.accepted).toBe(false)
    expect(result.task).toEqual(task)
    expect(service.listEvents(task.id).at(-1)).toMatchObject({ accepted: false })
    expect(messages.at(-1)).toMatchObject({ kind: 'task.updated', accepted: false })
  })

  it('lets a human override jump to another state', () => {
    const task = service.createTask({ title: 'First' })

    const result = service.applyEvent(task.id, {
      source: 'human',
      type: 'state.overridden',
      payload: { to: 'review' },
    })

    expect(result.task.state).toBe('review')
  })

  it('does not let a client send task.created', () => {
    const task = service.createTask({ title: 'First' })
    expect(() => service.applyEvent(task.id, { source: 'human', type: 'task.created' })).toThrow()
  })

  it('throws TaskNotFoundError for an unknown task', () => {
    expect(() => service.applyEvent('T-0099', { source: 'human', type: 'task.readied' })).toThrow(
      TaskNotFoundError,
    )
  })
})

describe('getTask and listEvents', () => {
  it('throws TaskNotFoundError for an unknown task', () => {
    expect(() => service.getTask('T-0099')).toThrow(TaskNotFoundError)
    expect(() => service.listEvents('T-0099')).toThrow(TaskNotFoundError)
  })
})

describe('recordActivity', () => {
  let clock: Date
  let activityService: TaskService
  let seen: DaemonMessage[]

  const later = (seconds: number) => {
    clock = new Date(clock.getTime() + seconds * 1000)
  }

  beforeEach(() => {
    clock = new Date('2026-10-08T09:00:00.000Z')
    seen = []
    const bus = new EventBus()
    bus.subscribe((message) => seen.push(message))
    activityService = new TaskService({ dataDir, index, bus, now: () => clock })
  })

  function startedTask() {
    const task = activityService.createTask({ title: 'Fix it' })
    activityService.applyEvent(task.id, { source: 'human', type: 'task.readied' })
    activityService.applyEvent(task.id, { source: 'agent', type: 'agent.started' })
    return task
  }

  it('moves the time of the last update of a running task and logs the sign of life', () => {
    const task = startedTask()
    later(31)

    expect(activityService.recordActivity(task.id)).toBe(true)

    expect(activityService.getTask(task.id).updatedAt).toBe('2026-10-08T09:00:31.000Z')
    expect(activityService.listEvents(task.id).at(-1)).toMatchObject({
      accepted: true,
      event: { type: 'agent.active', source: 'agent', at: '2026-10-08T09:00:31.000Z' },
    })
    expect(seen.at(-1)).toMatchObject({ kind: 'task.updated', accepted: true })
  })

  it('does not change the state of the task', () => {
    const task = startedTask()
    later(60)
    activityService.recordActivity(task.id)
    expect(activityService.getTask(task.id).state).toBe('running')
  })

  it('writes nothing again before the interval has passed', () => {
    const task = startedTask()
    later(31)
    activityService.recordActivity(task.id)
    const count = activityService.listEvents(task.id).length

    later(10)

    expect(activityService.recordActivity(task.id)).toBe(false)
    expect(activityService.listEvents(task.id)).toHaveLength(count)
  })

  it('uses the interval it is given', () => {
    const task = startedTask()
    later(5)

    expect(activityService.recordActivity(task.id, 60_000)).toBe(false)
    expect(activityService.recordActivity(task.id, 1000)).toBe(true)
  })

  it.each(['draft', 'ready', 'review'])('does nothing for a task that is %s', (state) => {
    const task = activityService.createTask({ title: 'Fix it' })
    activityService.applyEvent(task.id, {
      source: 'human',
      type: 'state.overridden',
      payload: { to: state },
    })
    later(3600)

    expect(activityService.recordActivity(task.id)).toBe(false)
  })

  it('throws TaskNotFoundError for an unknown task', () => {
    expect(() => activityService.recordActivity('T-0099')).toThrow(TaskNotFoundError)
  })
})

describe('init', () => {
  it('rebuilds the index from the task files', () => {
    service.createTask({ title: 'First' })
    service.createTask({ title: 'Second' })

    const freshIndex = new TaskIndex(':memory:')
    const freshService = makeService(freshIndex)
    expect(freshService.listTasks()).toEqual([])

    freshService.init()

    expect(
      freshService
        .listTasks()
        .map((task) => task.title)
        .sort(),
    ).toEqual(['First', 'Second'])
    freshIndex.close()
  })
})

describe('recordGit', () => {
  it('logs a commit and a push without changing the state or the pull request', () => {
    const task = service.createTask({ title: 'Fix it' })

    service.recordGit(task.id, 'git.committed', { hash: 'abc', subject: 'Fix it' })
    const pushed = service.recordGit(task.id, 'git.pushed', { branch: 'helmsman/t-0001' })

    expect(pushed).toMatchObject({ state: 'draft', pr: null })
    expect(service.listEvents(task.id).map((entry) => entry.event)).toMatchObject([
      { type: 'task.created' },
      { type: 'git.committed', source: 'git', payload: { hash: 'abc', subject: 'Fix it' } },
      { type: 'git.pushed', source: 'git', payload: { branch: 'helmsman/t-0001' } },
    ])
  })

  it('keeps the address of a new pull request on the task and in its file', () => {
    const task = service.createTask({ title: 'Fix it' })
    const url = 'https://github.com/owner/repo/pull/7'

    const updated = service.recordGit(task.id, 'pr.opened', { url })

    expect(updated.pr).toBe(url)
    expect(readTaskFile(dataDir, task.id)?.pr).toBe(url)
    expect(messages.at(-1)).toMatchObject({ kind: 'task.updated', task: { pr: url } })
  })
})

describe('updateTask', () => {
  it('changes the details and logs which fields changed', () => {
    const task = service.createTask({ title: 'First' })

    const updated = service.updateTask(task.id, {
      title: 'Better title',
      description: 'Notes',
      priority: 'high',
      labels: ['ui'],
    })

    expect(updated).toMatchObject({
      title: 'Better title',
      description: 'Notes',
      priority: 'high',
      labels: ['ui'],
      state: 'draft',
    })
    expect(readTaskFile(dataDir, task.id)).toEqual(updated)
    expect(service.listEvents(task.id).at(-1)?.event).toMatchObject({
      type: 'task.edited',
      source: 'human',
      payload: { fields: ['title', 'description', 'priority', 'labels'] },
    })
    expect(messages.at(-1)).toMatchObject({ kind: 'task.updated', task: { title: 'Better title' } })
  })

  it('writes nothing when nothing changed', () => {
    const task = service.createTask({ title: 'First', labels: ['a'] })
    const count = service.listEvents(task.id).length

    expect(service.updateTask(task.id, { title: 'First', labels: ['a'] })).toEqual(task)
    expect(service.listEvents(task.id)).toHaveLength(count)
  })

  it.each([
    [{ title: '  ' }],
    [{ priority: 'someday' }],
    [{ state: 'done' }],
    [{ workspace: '/tmp' }],
  ])('refuses %j', (input) => {
    const task = service.createTask({ title: 'First' })
    expect(() => service.updateTask(task.id, input)).toThrow()
  })

  it('refuses a task that depends on itself', () => {
    const task = service.createTask({ title: 'First' })
    expect(() => service.updateTask(task.id, { dependsOn: [task.id] })).toThrow(/itself/)
  })
})
