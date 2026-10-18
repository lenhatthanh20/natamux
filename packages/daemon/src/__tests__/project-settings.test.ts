import { mkdtempSync, readFileSync, rmSync, utimesSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { DEFAULT_PROJECT_SETTINGS } from '@natamux/core'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import {
  ProjectSettingsService,
  SETTINGS_FILE,
  SettingsChangedError,
  SettingsInputError,
} from '../project-settings.js'
import { RepoNotFoundError } from '../repo-service.js'

let folder: string
let clock: number
let service: ProjectSettingsService

// The service looks at the file at most once a second.
const later = (ms = 1000) => {
  clock += ms
}

beforeEach(() => {
  folder = mkdtempSync(join(tmpdir(), 'natamux-settings-'))
  clock = 0
  service = new ProjectSettingsService({ repos: { pathOf: () => folder }, now: () => clock })
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
      text: null,
      version: 'none',
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
    later()
    const loaded = service.load(null)
    expect(loaded.error).toContain('stages')
    expect(loaded.settings).toEqual(DEFAULT_PROJECT_SETTINGS)
  })

  it('reads the file again after it changes', () => {
    writeFileSync(file(), JSON.stringify({ tracker: { createIssue: 'never' } }))
    expect(service.get(null).tracker.createIssue).toBe('never')
    writeFileSync(file(), JSON.stringify({ tracker: { createIssue: 'on-pr' } }))
    utimesSync(file(), new Date(), new Date(Date.now() + 5000))
    later()
    expect(service.get(null).tracker.createIssue).toBe('on-pr')
  })

  it('looks at the file at most once a second, but sees its own save at once', () => {
    writeFileSync(file(), JSON.stringify({ tracker: { createIssue: 'never' } }))
    expect(service.get(null).tracker.createIssue).toBe('never')
    writeFileSync(file(), JSON.stringify({ tracker: { createIssue: 'on-pr' } }))
    utimesSync(file(), new Date(), new Date(Date.now() + 5000))

    later(999)
    expect(service.get(null).tracker.createIssue).toBe('never')
    later(1)
    expect(service.get(null).tracker.createIssue).toBe('on-pr')

    service.save(null, { tracker: { createIssue: 'never' } })
    expect(service.get(null).tracker.createIssue).toBe('never')
  })

  it('sees a file that was added or deleted after a second', () => {
    expect(service.load(null).found).toBe(false)
    writeFileSync(file(), '{}\n')
    expect(service.load(null).found).toBe(false)
    later()
    expect(service.load(null).found).toBe(true)

    rmSync(file())
    later()
    expect(service.load(null).found).toBe(false)
  })

  it('keeps a second for each repository on its own', () => {
    const other = mkdtempSync(join(tmpdir(), 'natamux-settings-'))
    try {
      const two = new ProjectSettingsService({
        repos: { pathOf: (repo) => (repo === 'other' ? other : folder) },
        now: () => clock,
      })
      expect(two.load(null).found).toBe(false)
      writeFileSync(join(other, SETTINGS_FILE), '{}\n')
      expect(two.load('other').found).toBe(true)
    } finally {
      rmSync(other, { recursive: true, force: true })
    }
  })

  it('saves valid settings as written, and refuses others', () => {
    const raw = { stages: [{ id: 'implement', kind: 'agent', gate: 'auto' }] }
    expect(service.save(null, raw).stages).toEqual(raw.stages)
    expect(JSON.parse(readFileSync(file(), 'utf8'))).toEqual(raw)
    expect(() => service.save(null, { stages: [] })).toThrow(SettingsInputError)
    expect(() => service.save(null, { agents: { permissionMode: 'yolo' } })).toThrow(
      'agents.permissionMode',
    )
  })

  it('gives the text of the file and a version that changes with it', () => {
    const text = '{ "tracker": { "createIssue": "never" }, "team": "web" }\n'
    writeFileSync(file(), text)
    const first = service.load(null)
    expect(first.text).toBe(text)
    expect(first.version).not.toBe('none')

    writeFileSync(file(), '{}\n')
    utimesSync(file(), new Date(), new Date(Date.now() + 5000))
    later()
    expect(service.load(null).version).not.toBe(first.version)
  })

  it('refuses a save over a file that changed since its version was read', () => {
    writeFileSync(file(), '{}\n')
    const { version } = service.load(null)
    writeFileSync(file(), '{ "tracker": { "createIssue": "never" } }\n')

    expect(() => service.save(null, { agents: { permissionMode: 'ask' } }, version)).toThrow(
      SettingsChangedError,
    )
    expect(readFileSync(file(), 'utf8')).toContain('never')
    // Without a version the save replaces the file, as "Overwrite" does.
    service.save(null, { agents: { permissionMode: 'ask' } })
    expect(readFileSync(file(), 'utf8')).toContain('ask')
  })

  it('takes the version of a missing file, and refuses it once the file is there', () => {
    service.save(null, {}, 'none')
    expect(() => service.save(null, {}, 'none')).toThrow(SettingsChangedError)
  })

  it('refuses a save over a file that was deleted, and allows one over a file only touched', () => {
    writeFileSync(file(), '{}\n')
    const { version } = service.load(null)
    utimesSync(file(), new Date(), new Date(Date.now() + 5000))
    expect(() => service.save(null, {}, version)).not.toThrow()

    const saved = service.load(null).version
    rmSync(file())
    expect(() => service.save(null, {}, saved)).toThrow(SettingsChangedError)
  })

  it('says why an empty file or one that is not an object cannot be used', () => {
    writeFileSync(file(), '')
    expect(service.load(null).error).toContain('is not JSON')
    writeFileSync(file(), '[]')
    utimesSync(file(), new Date(), new Date(Date.now() + 5000))
    later()
    expect(service.load(null).error).not.toBeNull()
    expect(service.load(null).settings).toEqual(DEFAULT_PROJECT_SETTINGS)
  })

  it('keeps the fields of a stage that the screen does not show', () => {
    const stages = [
      { id: 'implement', kind: 'agent', gate: 'auto', requireCommit: true, check: 'make test' },
      { id: 'qa', kind: 'manual', gate: 'manual', owner: 'sam' },
    ]
    service.save(null, { stages })
    expect(JSON.parse(readFileSync(file(), 'utf8')).stages).toEqual(stages)
  })

  it('keeps fields it does not know', () => {
    service.save(null, { team: 'web', tracker: { createIssue: 'never' } })
    expect(JSON.parse(readFileSync(file(), 'utf8'))).toEqual({
      team: 'web',
      tracker: { createIssue: 'never' },
    })
  })

  it('starts agents in auto mode when the file has a bad mode', () => {
    writeFileSync(file(), JSON.stringify({ agents: { permissionMode: 'yolo' } }))
    const loaded = service.load(null)
    expect(loaded.error).toContain('agents.permissionMode')
    expect(loaded.settings.agents.permissionMode).toBe('auto')
  })

  it('gives tasks of a removed repository the defaults, but does not load or save it', () => {
    const removed = new ProjectSettingsService({
      repos: {
        pathOf: (repo) => {
          throw new RepoNotFoundError(repo ?? 'default')
        },
      },
    })
    expect(removed.get('gone')).toEqual(DEFAULT_PROJECT_SETTINGS)
    expect(() => removed.load('gone')).toThrow(RepoNotFoundError)
    expect(() => removed.save('gone', {})).toThrow(RepoNotFoundError)
  })
})
