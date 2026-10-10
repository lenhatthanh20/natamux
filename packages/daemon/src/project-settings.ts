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
}

export class SettingsInputError extends Error {
  constructor(message: string) {
    super(message)
    this.name = 'SettingsInputError'
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

type Cached = { mtimeMs: number; loaded: LoadedSettings }

// `.natamux.json` of each repository. Read again when the file changes, so an edit in the
// editor counts at once.
export class ProjectSettingsService {
  private readonly repos: Pick<RepoService, 'pathOf'>
  private readonly cache = new Map<string, Cached>()

  constructor(options: { repos: Pick<RepoService, 'pathOf'> }) {
    this.repos = options.repos
  }

  pathOf(repo: string | null): string {
    return join(this.repos.pathOf(repo), SETTINGS_FILE)
  }

  load(repo: string | null): LoadedSettings {
    const path = this.pathOf(repo)
    let mtimeMs: number
    try {
      mtimeMs = statSync(path).mtimeMs
    } catch {
      return { settings: DEFAULT_PROJECT_SETTINGS, error: null, found: false }
    }
    const cached = this.cache.get(path)
    if (cached !== undefined && cached.mtimeMs === mtimeMs) {
      return cached.loaded
    }
    let loaded: LoadedSettings
    try {
      loaded = { settings: parseSettings(readFileSync(path, 'utf8')), error: null, found: true }
    } catch (error) {
      loaded = { settings: DEFAULT_PROJECT_SETTINGS, error: describe(error), found: true }
    }
    this.cache.set(path, { mtimeMs, loaded })
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

  // Writes the file in the repository's main folder. The person commits it.
  save(repo: string | null, raw: unknown): ProjectSettings {
    const parsed = projectSettingsSchema.safeParse(raw)
    if (!parsed.success) {
      throw new SettingsInputError(describe(parsed.error))
    }
    const path = this.pathOf(repo)
    const temporary = `${path}.tmp`
    writeFileSync(temporary, `${JSON.stringify(raw, null, 2)}\n`)
    renameSync(temporary, path)
    this.cache.delete(path)
    return parsed.data
  }
}
