import type { Task, TaskEvent } from '@helmsman/core'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { TaskIndex } from './task-index.js'

function makeTask(overrides: Partial<Task>): Task {
  return {
    id: 'T-0001',
    title: 'Example',
    description: '',
    state: 'draft',
    priority: 'normal',
    labels: [],
    agent: null,
    branch: null,
    workspace: null,
    pr: null,
    prStatus: null,
    parentId: null,
    repo: null,
    dependsOn: [],
    createdAt: '2026-10-08T09:00:00.000Z',
    updatedAt: '2026-10-08T09:00:00.000Z',
    ...overrides,
  }
}

let index: TaskIndex

beforeEach(() => {
  index = new TaskIndex(':memory:')
})

afterEach(() => {
  index.close()
})

describe('TaskIndex tasks', () => {
  it('lists the newest updated task first', () => {
    index.upsertTask(makeTask({ id: 'T-0001', updatedAt: '2026-10-08T09:00:00.000Z' }))
    index.upsertTask(makeTask({ id: 'T-0002', updatedAt: '2026-10-08T10:00:00.000Z' }))
    expect(index.listTasks().map((task) => task.id)).toEqual(['T-0002', 'T-0001'])
  })

  it('filters by state', () => {
    index.upsertTask(makeTask({ id: 'T-0001', state: 'running' }))
    index.upsertTask(makeTask({ id: 'T-0002', state: 'review' }))
    expect(index.listTasks('review').map((task) => task.id)).toEqual(['T-0002'])
  })

  it('replaces a task that is upserted again', () => {
    index.upsertTask(makeTask({ title: 'Old' }))
    index.upsertTask(makeTask({ title: 'New' }))
    expect(index.listTasks().map((task) => task.title)).toEqual(['New'])
  })

  it('replaces every task at once', () => {
    index.upsertTask(makeTask({ id: 'T-0001' }))
    index.replaceAllTasks([makeTask({ id: 'T-0005' }), makeTask({ id: 'T-0006' })])
    expect(
      index
        .listTasks()
        .map((task) => task.id)
        .sort(),
    ).toEqual(['T-0005', 'T-0006'])
  })
})

describe('TaskIndex events', () => {
  const event: TaskEvent = {
    taskId: 'T-0001',
    source: 'agent',
    type: 'agent.asked',
    at: '2026-10-08T09:05:00.000Z',
    payload: { question: 'Which branch?' },
  }

  it('lists events of a task in the order they were added, with the accepted flag', () => {
    index.appendEvent(event, true)
    index.appendEvent({ ...event, type: 'agent.failed' }, false)
    index.appendEvent({ ...event, taskId: 'T-0002' }, true)

    const stored = index.listEvents('T-0001')

    expect(stored.map((entry) => [entry.event.type, entry.accepted])).toEqual([
      ['agent.asked', true],
      ['agent.failed', false],
    ])
    expect(stored[0]?.event.payload).toEqual({ question: 'Which branch?' })
    expect(stored[0]?.seq).toBeLessThan(stored[1]?.seq ?? 0)
  })

  it('returns no events for an unknown task', () => {
    expect(index.listEvents('T-0099')).toEqual([])
  })
})
