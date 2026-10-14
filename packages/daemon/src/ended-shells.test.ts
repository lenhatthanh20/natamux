import { existsSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { ENDED_SHELL_MAX_CHARS } from '@natamux/core'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import {
  deleteEndedShells,
  endedShellsPath,
  readEndedShells,
  saveEndedShells,
} from './ended-shells.js'

let dataDir: string

beforeEach(() => {
  dataDir = mkdtempSync(join(tmpdir(), 'natamux-ended-'))
})

afterEach(() => {
  rmSync(dataDir, { recursive: true, force: true })
})

const shell = {
  name: '1',
  agent: 'claude-code',
  stage: 'implement',
  endedAt: '2026-10-15T00:07:00.000Z',
  text: 'All checks passed.',
}

describe('ended shells', () => {
  it('keeps the shells beside the task file, not with its stage files', () => {
    saveEndedShells(dataDir, 'T-0058', [shell, { ...shell, name: '2', agent: null }])

    expect(endedShellsPath(dataDir, 'T-0058')).toBe(join(dataDir, 'tasks', 'T-0058.shells.json'))
    expect(readEndedShells(dataDir, 'T-0058')).toEqual([
      shell,
      { ...shell, name: '2', agent: null },
    ])
  })

  it('keeps the end of a long screen', () => {
    const text = `${'a'.repeat(ENDED_SHELL_MAX_CHARS)}the end`

    saveEndedShells(dataDir, 'T-0058', [{ ...shell, text }])

    const [saved] = readEndedShells(dataDir, 'T-0058')
    expect(saved?.text).toHaveLength(ENDED_SHELL_MAX_CHARS)
    expect(saved?.text.endsWith('the end')).toBe(true)
  })

  it('reads nothing for a task that has no file, or a file it cannot read', () => {
    expect(readEndedShells(dataDir, 'T-0001')).toEqual([])

    saveEndedShells(dataDir, 'T-0002', [shell])
    writeFileSync(endedShellsPath(dataDir, 'T-0002'), '{ not json')
    expect(readEndedShells(dataDir, 'T-0002')).toEqual([])
  })

  it('reads a file from before stages were kept', () => {
    saveEndedShells(dataDir, 'T-0058', [shell])
    const { stage: _, ...old } = shell
    writeFileSync(endedShellsPath(dataDir, 'T-0058'), JSON.stringify([old]))

    expect(readEndedShells(dataDir, 'T-0058')).toEqual([{ ...shell, stage: null }])
  })

  it('deletes the file, and a missing file is no error', () => {
    saveEndedShells(dataDir, 'T-0058', [shell])

    deleteEndedShells(dataDir, 'T-0058')
    deleteEndedShells(dataDir, 'T-0058')

    expect(existsSync(endedShellsPath(dataDir, 'T-0058'))).toBe(false)
  })
})
