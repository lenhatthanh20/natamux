import { mkdirSync, mkdtempSync, rmSync, statSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import type { Task } from '@natamux/core'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import {
  listTaskFiles,
  listTaskIds,
  parseTaskFile,
  readTaskFile,
  sameStat,
  serializeTask,
  statFile,
  taskFilePath,
  tasksDir,
  watchTaskFiles,
  writeTaskFile,
} from './task-files.js'

const task: Task = {
  id: 'T-0042',
  title: 'Fix timezone bug in reminders',
  description: 'The reminder fires one hour late.\n\nSee the run notes.',
  state: 'running',
  priority: 'high',
  labels: ['bug'],
  agent: 'claude-code',
  branch: 'natamux/t-0042-fix-timezone',
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
  dependsOn: ['T-0007'],
  createdAt: '2026-10-08T09:00:00.000Z',
  updatedAt: '2026-10-08T09:30:00.000Z',
}

let dataDir: string

beforeEach(() => {
  dataDir = mkdtempSync(join(tmpdir(), 'natamux-files-'))
})

afterEach(() => {
  rmSync(dataDir, { recursive: true, force: true })
})

describe('task files', () => {
  it('reads back exactly what it wrote', () => {
    writeTaskFile(dataDir, task)
    expect(readTaskFile(dataDir, 'T-0042')).toEqual(task)
  })

  it('round-trips an empty description', () => {
    const empty = { ...task, description: '' }
    expect(parseTaskFile(serializeTask(empty))).toEqual(empty)
  })

  it('reads a hand-written file with an unquoted timestamp', () => {
    const text = [
      '---',
      'id: T-0001',
      'title: Written by hand',
      'createdAt: 2026-10-08T09:00:00Z',
      'updatedAt: 2026-10-08T09:00:00Z',
      '---',
      'A note.',
    ].join('\n')

    const parsed = parseTaskFile(text)

    expect(parsed.createdAt).toBe('2026-10-08T09:00:00.000Z')
    expect(parsed.description).toBe('A note.')
    expect(parsed.state).toBe('draft')
  })

  it('throws on a file that is not a valid task', () => {
    expect(() => parseTaskFile('---\nid: nope\n---\n')).toThrow()
  })

  it('returns undefined for a task that does not exist', () => {
    expect(readTaskFile(dataDir, 'T-0099')).toBeUndefined()
  })

  it('lists task ids in numeric order and ignores other files', () => {
    mkdirSync(tasksDir(dataDir), { recursive: true })
    const names = ['T-10.md', 'T-2.md', 'T-3.md.tmp', 'notes.txt', 'T-x.md', 'abc-1.md', 'T42.md']
    for (const name of [...names, 'A1-2.md', 'TASK-5.md.bak', 'TASK-5.md', 'ABC-2.md']) {
      writeFileSync(join(tasksDir(dataDir), name), '')
    }
    expect(listTaskIds(dataDir)).toEqual(['ABC-2', 'T-2', 'TASK-5', 'T-10'])
  })

  it('lists no ids when the data directory has no tasks yet', () => {
    expect(listTaskIds(join(dataDir, 'missing'))).toEqual([])
  })

  it('lists every task file as a task', () => {
    writeTaskFile(dataDir, task)
    writeTaskFile(dataDir, { ...task, id: 'T-0043', title: 'Second' })
    expect(listTaskFiles(dataDir).map((listed) => listed.id)).toEqual(['T-0042', 'T-0043'])
  })

  it('lists tasks with ids the user chose, by the number at the end of the id', () => {
    for (const id of ['T-0010', 'ABC-12', 'TASK-0005', 'T-0003', 'T-0005']) {
      writeTaskFile(dataDir, { ...task, id })
    }
    expect(listTaskFiles(dataDir).map((listed) => listed.id)).toEqual([
      'T-0003',
      'T-0005',
      'TASK-0005',
      'T-0010',
      'ABC-12',
    ])
  })
})

describe('file stats', () => {
  it('gives the time and size of the file it wrote', () => {
    const written = writeTaskFile(dataDir, task)
    const { mtimeMs, size } = statSync(taskFilePath(dataDir, task.id))

    expect(written).toEqual({ mtimeMs, size })
    expect(statFile(taskFilePath(dataDir, task.id))).toEqual(written)
    expect(statFile(taskFilePath(dataDir, 'T-0099'))).toBeUndefined()
  })

  it('counts a file as changed when its time or its size differs', () => {
    expect(sameStat({ mtimeMs: 1, size: 2 }, { mtimeMs: 1, size: 2 })).toBe(true)
    expect(sameStat({ mtimeMs: 1, size: 2 }, { mtimeMs: 1, size: 3 })).toBe(false)
    expect(sameStat({ mtimeMs: 1, size: 2 }, { mtimeMs: 4, size: 2 })).toBe(false)
    expect(sameStat({ mtimeMs: 1, size: 2 }, undefined)).toBe(false)
    expect(sameStat(undefined, undefined)).toBe(false)
  })
})

describe('watchTaskFiles', () => {
  it('reports the ids of task files that change, a burst of writes at a time', async () => {
    mkdirSync(tasksDir(dataDir), { recursive: true })
    const reports: string[][] = []
    const stop = watchTaskFiles(dataDir, (ids) => reports.push([...ids]))
    try {
      writeFileSync(join(tasksDir(dataDir), 'T-0043.events.jsonl'), '{}\n')
      writeFileSync(join(tasksDir(dataDir), 'T-0043.md.tmp'), 'draft')
      for (let count = 0; count < 5; count += 1) {
        writeFileSync(taskFilePath(dataDir, task.id), `edit ${count}`)
      }

      await vi.waitFor(() => expect(reports.flat()).toContain(task.id), { timeout: 2000 })
      expect(reports.flat().every((id) => id === task.id)).toBe(true)
      expect(reports.length).toBeLessThan(5)
    } finally {
      stop()
    }
  })

  it('makes the folder it watches when there is none yet', () => {
    const stop = watchTaskFiles(dataDir, () => {})
    stop()
    expect(statFile(tasksDir(dataDir))).toBeDefined()
  })
})
