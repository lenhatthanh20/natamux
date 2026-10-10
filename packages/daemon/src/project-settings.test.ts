import { mkdtempSync, readFileSync, rmSync, utimesSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { DEFAULT_PROJECT_SETTINGS } from '@natamux/core'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { ProjectSettingsService, SETTINGS_FILE, SettingsInputError } from './project-settings.js'

let folder: string
let service: ProjectSettingsService

beforeEach(() => {
  folder = mkdtempSync(join(tmpdir(), 'natamux-settings-'))
  service = new ProjectSettingsService({ repos: { pathOf: () => folder } })
})

afterEach(() => {
  rmSync(folder, { recursive: true, force: true })
})

const file = () => join(folder, SETTINGS_FILE)

describe('ProjectSettingsService', () => {
  it('uses the defaults without a file', () => {
    expect(service.load(null)).toEqual({
      settings: DEFAULT_PROJECT_SETTINGS,
      error: null,
      found: false,
    })
  })

  it('reads a file and fills what it leaves out', () => {
    writeFileSync(file(), JSON.stringify({ tracker: { createIssue: 'never' } }))
    const loaded = service.load(null)
    expect(loaded.error).toBeNull()
    expect(loaded.settings.tracker).toEqual({
      type: 'github',
      createIssue: 'never',
      linkInPr: true,
    })
    expect(loaded.settings.stages).toEqual(DEFAULT_PROJECT_SETTINGS.stages)
  })

  it('says why a file cannot be used, and keeps the defaults', () => {
    writeFileSync(file(), '{ not json')
    expect(service.load(null).error).toContain('is not JSON')

    writeFileSync(file(), JSON.stringify({ stages: [] }))
    utimesSync(file(), new Date(), new Date(Date.now() + 5000))
    const loaded = service.load(null)
    expect(loaded.error).toContain('stages')
    expect(loaded.settings).toEqual(DEFAULT_PROJECT_SETTINGS)
  })

  it('reads the file again after it changes', () => {
    writeFileSync(file(), JSON.stringify({ tracker: { createIssue: 'never' } }))
    expect(service.get(null).tracker.createIssue).toBe('never')
    writeFileSync(file(), JSON.stringify({ tracker: { createIssue: 'on-pr' } }))
    utimesSync(file(), new Date(), new Date(Date.now() + 5000))
    expect(service.get(null).tracker.createIssue).toBe('on-pr')
  })

  it('saves valid settings as written, and refuses others', () => {
    const raw = { stages: [{ id: 'implement', kind: 'agent', gate: 'auto' }] }
    expect(service.save(null, raw).stages).toEqual(raw.stages)
    expect(JSON.parse(readFileSync(file(), 'utf8'))).toEqual(raw)
    expect(() => service.save(null, { stages: [] })).toThrow(SettingsInputError)
  })
})
