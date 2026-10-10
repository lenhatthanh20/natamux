import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'

export type DesktopSettings = {
  // The repository older versions asked for on the first run. The app no longer writes it.
  repoDir?: string
  // False turns the timed update checks off. Missing means on.
  autoUpdate?: boolean
}

export function loadSettings(file: string): DesktopSettings {
  try {
    const parsed: unknown = JSON.parse(readFileSync(file, 'utf8'))
    if (typeof parsed === 'object' && parsed !== null) {
      const { repoDir, autoUpdate } = parsed as Record<string, unknown>
      return {
        ...(typeof repoDir === 'string' && repoDir !== '' ? { repoDir } : {}),
        ...(typeof autoUpdate === 'boolean' ? { autoUpdate } : {}),
      }
    }
  } catch {
    // No settings yet, or a file that is not JSON: start over.
  }
  return {}
}

export function saveSettings(file: string, settings: DesktopSettings): void {
  mkdirSync(dirname(file), { recursive: true })
  writeFileSync(file, `${JSON.stringify(settings, null, 2)}\n`)
}

// A git repository has .git at its top: a folder, or a file in a worktree.
export function isGitRepository(dir: string): boolean {
  return existsSync(join(dir, '.git'))
}

// The daemon adds this repository once as the first project, so the tasks of an older version
// keep it. Undefined: the daemon starts with no project.
export function startingRepoDir(settings: DesktopSettings): string | undefined {
  return settings.repoDir !== undefined && isGitRepository(settings.repoDir)
    ? settings.repoDir
    : undefined
}
