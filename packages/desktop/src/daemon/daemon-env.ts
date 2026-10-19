import { existsSync } from 'node:fs'
import { delimiter } from 'node:path'

// Where Homebrew and most installers put commands. Used when the login shell gave no PATH.
const FALLBACK_BIN_DIRS = ['/opt/homebrew/bin', '/opt/homebrew/sbin', '/usr/local/bin']

export type DaemonPlace = {
  port: number
  dataDir: string
  // The repository to add once as the first project. Undefined: none.
  repoDir: string | undefined
  webDir: string
  // Holds the `natamux` command for the agents' shells.
  binDir: string
  // The PATH of the user's login shell. An app started from the Dock or Finder gets a short PATH
  // without Homebrew, nvm and the like, where the agents are installed.
  loginPath: string | undefined
}

export function fallbackBinDirs(
  platform: NodeJS.Platform = process.platform,
  exists: (folder: string) => boolean = existsSync,
): string[] {
  return platform === 'win32' ? [] : FALLBACK_BIN_DIRS.filter((folder) => exists(folder))
}

// The environment of the daemon. Its shells and agents inherit it.
export function daemonEnv(
  base: NodeJS.ProcessEnv,
  daemonToken: string,
  place: DaemonPlace,
  fallbackDirs: string[] = fallbackBinDirs(),
): NodeJS.ProcessEnv {
  const parts = [place.binDir, ...(place.loginPath ?? base.PATH ?? '').split(delimiter)]
  if (place.loginPath === undefined) {
    parts.push(...fallbackDirs)
  }
  const path = [...new Set(parts.filter((part) => part !== ''))].join(delimiter)
  const env: NodeJS.ProcessEnv = {
    ...base,
    PATH: path,
    NATAMUX_HOST: '127.0.0.1',
    NATAMUX_PORT: String(place.port),
    NATAMUX_TOKEN: daemonToken,
    NATAMUX_DATA_DIR: place.dataDir,
    NATAMUX_WEB_DIR: place.webDir,
    NATAMUX_BIN_DIR: place.binDir,
  }
  if (place.repoDir === undefined) {
    delete env.NATAMUX_REPO_DIR
  } else {
    env.NATAMUX_REPO_DIR = place.repoDir
  }
  // Left set, it would make every Electron app the agents start run as plain Node.js.
  delete env.ELECTRON_RUN_AS_NODE
  return env
}
