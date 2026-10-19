import { mkdirSync, mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import type { TaskEvent } from '@natamux/core'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { appendEventLine, eventLogPath, readEventLog } from '../event-log.js'
import { statFile, tasksDir } from '../task-files.js'

// Counts the folders the log makes. Everything else is the real file system.
vi.mock('node:fs', async (importOriginal) => {
  const actual = await importOriginal<typeof import('node:fs')>()
  return { ...actual, mkdirSync: vi.fn(actual.mkdirSync) }
})

const event: TaskEvent = {
  taskId: 'T-0001',
  source: 'agent',
  type: 'agent.active',
  at: '2026-10-08T09:00:00.000Z',
  payload: {},
}

let dataDir: string

beforeEach(() => {
  dataDir = mkdtempSync(join(tmpdir(), 'natamux-log-'))
})

afterEach(() => {
  rmSync(dataDir, { recursive: true, force: true })
})

describe('appendEventLine', () => {
  it('adds a line and gives the stat of the log after it', () => {
    const first = appendEventLine(dataDir, event, true)
    const second = appendEventLine(dataDir, { ...event, type: 'agent.failed' }, false)

    expect(second).toEqual(statFile(eventLogPath(dataDir, event.taskId)))
    expect(second.size).toBeGreaterThan(first.size)
    expect(readEventLog(dataDir, event.taskId)).toEqual([
      { event, accepted: true },
      { event: { ...event, type: 'agent.failed' }, accepted: false },
    ])
  })

  it('makes the tasks folder when there is none, also after it was removed', () => {
    appendEventLine(dataDir, event, true)
    rmSync(tasksDir(dataDir), { recursive: true })

    appendEventLine(dataDir, event, true)

    expect(readEventLog(dataDir, event.taskId)).toHaveLength(1)
  })

  it('makes the folder once for many events of many tasks', () => {
    vi.mocked(mkdirSync).mockClear()

    for (let count = 0; count < 100; count += 1) {
      appendEventLine(dataDir, event, true)
    }
    appendEventLine(dataDir, { ...event, taskId: 'T-0002' }, true)

    expect(mkdirSync).toHaveBeenCalledTimes(1)
    expect(readEventLog(dataDir, event.taskId)).toHaveLength(100)
    expect(readEventLog(dataDir, 'T-0002')).toHaveLength(1)
  })
})
