import { createHash } from 'node:crypto'
import { readFileSync, renameSync, statSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import {
  DEFAULT_PROJECT_SETTINGS,
  type ProjectSettings,
  projectSettingsSchema,
} from '@natamux/core'
import { ZodError } from 'zod'
import { RepoNotFoundError, type RepoService } from './repo-service.js'

export const SETTINGS_FILE = '.natamux.json'

export type LoadedSettings = {
  settings: ProjectSettings
  // Why the file could not be used. The defaults apply then.
  error: string | null
  // The file was there.
  found: boolean
  // The text of the file, so an editor can keep what it does not know.
  text: string | null
  // Changes when the file changes. A save that names an older version is refused.
  version: string
}

const NO_FILE = 'none'

function versionOf(text: string | null): string {
  return text === null ? NO_FILE : createHash('sha256').update(text).digest('hex').slice(0, 16)
}

export class SettingsInputError extends Error {
  constructor(message: string) {
    super(message)
    this.name = 'SettingsInputError'
  }
}

export class SettingsChangedError extends Error {
  constructor(path: string) {
    super(`${path} changed since you opened it. Reload it, or overwrite it.`)
    this.name = 'SettingsChangedError'
  }
}

function describe(error: unknown): string {
  if (error instanceof ZodError) {
    return error.issues
      .map((issue) => `${issue.path.join('.') || 'file'}: ${issue.message}`)
      .join('; ')
  }
  return error instanceof Error ? error.message : String(error)
}

// Parses the text of a settings file. Throws SettingsInputError with a short reason.
export function parseSettings(text: string): ProjectSettings {
  let raw: unknown
  try {
    raw = JSON.parse(text)
  } catch (error) {
    throw new SettingsInputError(`${SETTINGS_FILE} is not JSON: ${describe(error)}`)
  }
  const parsed = projectSettingsSchema.safeParse(raw)
  if (!parsed.success) {
    throw new SettingsInputError(describe(parsed.error))
  }
  return parsed.data
}

function readText(path: string): string | null {
  try {
    return readFileSync(path, 'utf8')
  } catch {
    return null
  }
}

const NOT_FOUND: LoadedSettings = {
  settings: DEFAULT_PROJECT_SETTINGS,
  error: null,
  found: false,
  text: null,
  version: NO_FILE,
}

// How long a look at the file counts. Hooks ask for the settings on every tool call.
const STAT_EVERY_MS = 1000

// `mtimeMs` is null for a file that is not there.
type Cached = { mtimeMs: number | null; checkedAt: number; loaded: LoadedSettings }

// `.natamux.json` of each repository. Read again when the file changes, so an edit in the
// editor counts within a second.
export class ProjectSettingsService {
  private readonly repos: Pick<RepoService, 'pathOf'>
  private readonly now: () => number
  private readonly cache = new Map<string, Cached>()

  constructor(options: { repos: Pick<RepoService, 'pathOf'>; now?: () => number }) {
    this.repos = options.repos
    this.now = options.now ?? Date.now
  }

  pathOf(repo: string | null): string {
    return join(this.repos.pathOf(repo), SETTINGS_FILE)
  }

  load(repo: string | null): LoadedSettings {
    const path = this.pathOf(repo)
    const now = this.now()
    const cached = this.cache.get(path)
    if (cached !== undefined && now - cached.checkedAt < STAT_EVERY_MS) {
      return cached.loaded
    }
    let mtimeMs: number | null
    try {
      mtimeMs = statSync(path).mtimeMs
    } catch {
      mtimeMs = null
    }
    const unchanged = cached !== undefined && cached.mtimeMs === mtimeMs
    const loaded = unchanged ? cached.loaded : this.read(path, mtimeMs)
    this.cache.set(path, { mtimeMs, checkedAt: now, loaded })
    return loaded
  }

  // Tasks keep the id of a removed repository. They still need stages to be moved, cancelled or
  // watched, so they get the defaults.
  get(repo: string | null): ProjectSettings {
    try {
      return this.load(repo).settings
    } catch (error) {
      if (error instanceof RepoNotFoundError) {
        return DEFAULT_PROJECT_SETTINGS
      }
      throw error
    }
  }

  // Writes the file in the repository's main folder. The person commits it. With `version`, the
  // file must not have changed since that version was read, so no one's edit is lost unseen.
  save(repo: string | null, raw: unknown, version?: string): ProjectSettings {
    const parsed = projectSettingsSchema.safeParse(raw)
    if (!parsed.success) {
      throw new SettingsInputError(describe(parsed.error))
    }
    const path = this.pathOf(repo)
    if (version !== undefined && versionOf(readText(path)) !== version) {
      throw new SettingsChangedError(path)
    }
    const temporary = `${path}.tmp`
    writeFileSync(temporary, `${JSON.stringify(raw, null, 2)}\n`)
    renameSync(temporary, path)
    this.cache.delete(path)
    return parsed.data
  }

  private read(path: string, mtimeMs: number | null): LoadedSettings {
    if (mtimeMs === null) {
      return NOT_FOUND
    }
    const text = readText(path)
    const file = { found: true, text, version: versionOf(text) }
    try {
      return { settings: parseSettings(text ?? ''), error: null, ...file }
    } catch (error) {
      return { settings: DEFAULT_PROJECT_SETTINGS, error: describe(error), ...file }
    }
  }
}
