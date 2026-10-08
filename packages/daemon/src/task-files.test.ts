import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import type { Task } from '@helmsman/core'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import {
  listTaskFiles,
  listTaskIds,
  parseTaskFile,
  readTaskFile,
  serializeTask,
  tasksDir,
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
  branch: 'helmsman/t-0042-fix-timezone',
  workspace: null,
  pr: null,
  prStatus: null,
  parentId: null,
  dependsOn: ['T-0007'],
  createdAt: '2026-10-08T09:00:00.000Z',
  updatedAt: '2026-10-08T09:30:00.000Z',
}

let dataDir: string

beforeEach(() => {
  dataDir = mkdtempSync(join(tmpdir(), 'helmsman-files-'))
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
    for (const name of ['T-10.md', 'T-2.md', 'T-3.md.tmp', 'notes.txt', 'T-x.md']) {
      writeFileSync(join(tasksDir(dataDir), name), '')
    }
    expect(listTaskIds(dataDir)).toEqual(['T-2', 'T-10'])
  })

  it('lists no ids when the data directory has no tasks yet', () => {
    expect(listTaskIds(join(dataDir, 'missing'))).toEqual([])
  })

  it('lists every task file as a task', () => {
    writeTaskFile(dataDir, task)
    writeTaskFile(dataDir, { ...task, id: 'T-0043', title: 'Second' })
    expect(listTaskFiles(dataDir).map((listed) => listed.id)).toEqual(['T-0042', 'T-0043'])
  })
})
