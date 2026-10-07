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
