import { existsSync } from 'node:fs'
import { join } from 'node:path'

// A repository can hold a script that prepares a new workspace: install packages, copy env files.
// It runs in the first terminal of the task, so its output can be read there.
export const SETUP_SCRIPT = '.helmsman-setup'
export const SETUP_SCRIPT_WINDOWS = '.helmsman-setup.ps1'

// The command line that runs the setup script of a worktree, or null when it has none.
export function setupCommand(
  worktreePath: string,
  platform: NodeJS.Platform = process.platform,
): string | null {
  if (platform === 'win32') {
    return existsSync(join(worktreePath, SETUP_SCRIPT_WINDOWS))
      ? `& ./${SETUP_SCRIPT_WINDOWS}`
      : null
  }
  return existsSync(join(worktreePath, SETUP_SCRIPT)) ? `sh ./${SETUP_SCRIPT}` : null
}
