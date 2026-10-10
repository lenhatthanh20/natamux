import { delimiter } from 'node:path'

export type DaemonPlace = {
  port: number
  dataDir: string
  repoDir: string
  webDir: string
  // Holds the `natamux` command for the agents' shells.
  binDir: string
  // The PATH of the user's login shell. An app started from the Dock or Finder gets a short PATH
  // without Homebrew, nvm and the like, where the agents are installed.
  loginPath: string | undefined
}

// The environment of the daemon. Its shells and agents inherit it.
export function daemonEnv(
  base: NodeJS.ProcessEnv,
  daemonToken: string,
  place: DaemonPlace,
): NodeJS.ProcessEnv {
  const path = [place.binDir, place.loginPath ?? base.PATH ?? '']
    .filter((part) => part !== '')
    .join(delimiter)
  const env: NodeJS.ProcessEnv = {
    ...base,
    PATH: path,
    NATAMUX_HOST: '127.0.0.1',
    NATAMUX_PORT: String(place.port),
    NATAMUX_TOKEN: daemonToken,
    NATAMUX_DATA_DIR: place.dataDir,
    NATAMUX_REPO_DIR: place.repoDir,
    NATAMUX_WEB_DIR: place.webDir,
  }
  // Left set, it would make every Electron app the agents start run as plain Node.js.
  delete env.ELECTRON_RUN_AS_NODE
  return env
}
