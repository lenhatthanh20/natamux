import { existsSync } from 'node:fs'

// Kept apart from pty.ts so tests can check it without loading the native node-pty module.
export function shellCommand(
  env: NodeJS.ProcessEnv,
  platform: NodeJS.Platform,
  exists: (file: string) => boolean = existsSync,
): { file: string; args: string[] } {
  if (platform === 'win32') {
    return { file: 'powershell.exe', args: [] }
  }
  // A login shell, like a Terminal window: many users set up the PATH (Homebrew, mise, nvm) in
  // ~/.zprofile or ~/.bash_profile, which only a login shell reads.
  if (env.SHELL && exists(env.SHELL)) {
    return { file: env.SHELL, args: ['-l'] }
  }
  return { file: exists('/bin/bash') ? '/bin/bash' : '/bin/sh', args: ['-l'] }
}
