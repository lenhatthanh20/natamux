import { mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { DatabaseSync } from 'node:sqlite'
import type { Task, TaskEvent } from '@natamux/core'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
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
    stage: 'implement',
    stages: ['implement', 'review', 'release'],
    lanes: {},
    paths: {},
    stageSince: null,
    backgroundSince: null,
    workflowStages: [],
    context: null,
    issue: null,
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

describe('TaskIndex record', () => {
  const event: TaskEvent = {
    taskId: 'T-0001',
    source: 'agent',
    type: 'agent.started',
    at: '2026-10-08T09:05:00.000Z',
    payload: {},
  }

  it('stores the task, its events and the stats of its files together', () => {
    const task = makeTask({ state: 'running' })

    index.record('T-0001', {
      task,
      events: [{ event, accepted: true }],
      files: { task: { mtimeMs: 1.5, size: 10 }, log: { mtimeMs: 2, size: 20 } },
    })

    expect(index.listTasks()).toEqual([task])
    expect(index.listEvents('T-0001').map((entry) => entry.accepted)).toEqual([true])
    expect(index.taskFiles().get('T-0001')).toEqual({
      task: { mtimeMs: 1.5, size: 10 },
      log: { mtimeMs: 2, size: 20 },
    })
  })

  it('keeps the stat of a file that a change did not write', () => {
    index.record('T-0001', { task: makeTask({}), files: { task: { mtimeMs: 1, size: 10 } } })
    index.record('T-0001', {
      events: [{ event, accepted: false }],
      files: { log: { mtimeMs: 2, size: 20 } },
    })

    expect(index.listEvents('T-0001').map((entry) => entry.accepted)).toEqual([false])
    expect(index.taskFiles().get('T-0001')).toEqual({
      task: { mtimeMs: 1, size: 10 },
      log: { mtimeMs: 2, size: 20 },
    })
  })

  it('stores nothing of a change that fails half way', () => {
    // JSON cannot hold a BigInt, so the event fails after the task row is written.
    const change = {
      task: makeTask({}),
      events: [{ event: { ...event, payload: { size: 1n } }, accepted: true }],
      files: { task: { mtimeMs: 1, size: 10 } },
    }

    expect(() => index.record('T-0001', change)).toThrow()

    expect(index.listTasks()).toEqual([])
    expect(index.listEvents('T-0001')).toEqual([])
    expect(index.taskFiles().size).toBe(0)
  })

  it('forgets the files of a deleted task', () => {
    index.record('T-0001', { task: makeTask({}), files: { task: { mtimeMs: 1, size: 10 } } })
    index.deleteTask('T-0001')
    expect(index.taskFiles().size).toBe(0)
  })

  it('replaces the stats of every file along with the tasks', () => {
    index.record('T-0001', { task: makeTask({}), files: { task: { mtimeMs: 1, size: 10 } } })

    index.replaceAllTasks(
      [makeTask({ id: 'T-0005' })],
      new Map([['T-0005', { task: { mtimeMs: 5, size: 50 } }]]),
    )

    expect([...index.taskFiles()]).toEqual([['T-0005', { task: { mtimeMs: 5, size: 50 } }]])
  })
})

describe('TaskIndex on the hot path', () => {
  const event: TaskEvent = {
    taskId: 'T-0001',
    source: 'agent',
    type: 'agent.active',
    at: '2026-10-08T09:05:00.000Z',
    payload: {},
  }

  it('writes ahead to a log and syncs to disk only at checkpoints', () => {
    const folder = mkdtempSync(join(tmpdir(), 'natamux-index-'))
    const onDisk = new TaskIndex(join(folder, 'index.sqlite'))
    try {
      // The setting belongs to the connection, which the index keeps to itself.
      const { db } = onDisk as unknown as { db: DatabaseSync }
      expect(db.prepare('pragma journal_mode').get()).toEqual({ journal_mode: 'wal' })
      // 1 is NORMAL. WAL alone would keep FULL (2).
      expect(db.prepare('pragma synchronous').get()).toEqual({ synchronous: 1 })
    } finally {
      onDisk.close()
      rmSync(folder, { recursive: true, force: true })
    }
  })

  it('prepares its statements once, not on every call', () => {
    const prepare = vi.spyOn(DatabaseSync.prototype, 'prepare')
    const own = new TaskIndex(':memory:')
    try {
      const prepared = prepare.mock.calls.length
      expect(prepared).toBeGreaterThan(0)

      for (let count = 0; count < 10; count += 1) {
        own.record('T-0001', {
          task: makeTask({}),
          events: [{ event, accepted: true }],
          files: { task: { mtimeMs: count, size: 10 }, log: { mtimeMs: count, size: 20 } },
        })
        own.listTasks()
        own.listTasks('draft')
        own.listEvents('T-0001')
        own.countEvents('T-0001')
        own.taskFiles()
      }
      own.replaceEvents('T-0001', [{ event, accepted: false }])
      own.replaceAllTasks([makeTask({})], new Map([['T-0001', { task: { mtimeMs: 1, size: 1 } }]]))
      own.deleteTask('T-0001')

      expect(prepare).toHaveBeenCalledTimes(prepared)
    } finally {
      own.close()
      prepare.mockRestore()
    }
  })

  it('records a change in exactly one transaction', () => {
    const exec = vi.spyOn(DatabaseSync.prototype, 'exec')
    try {
      index.record('T-0001', {
        task: makeTask({}),
        events: [
          { event, accepted: true },
          { event, accepted: false },
        ],
        files: { task: { mtimeMs: 1, size: 10 }, log: { mtimeMs: 2, size: 20 } },
      })

      expect(exec.mock.calls).toEqual([['begin'], ['commit']])
    } finally {
      exec.mockRestore()
    }
  })
})
