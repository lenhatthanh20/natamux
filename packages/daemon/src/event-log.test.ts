import { mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import type { TaskEvent } from '@natamux/core'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { appendEventLine, eventLogPath, readEventLog } from './event-log.js'
import { statFile, tasksDir } from './task-files.js'

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
})
