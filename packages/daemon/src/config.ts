import { randomBytes } from 'node:crypto'
import { join } from 'node:path'

export const DEFAULT_PORT = 4317
export const LOOPBACK_HOST = '127.0.0.1'

export type DaemonConfig = {
  host: string
  port: number
  dataDir: string
  repoDir: string
  worktreesDir: string
  // The built dashboard's index.html, for deep links. Undefined: the daemon serves no page.
  webIndexPath: string | undefined
  // The built dashboard, served by the daemon (the desktop app, or a daemon without a web server).
  webDir: string | undefined
  token: string
  tokenWasGenerated: boolean
}

export function loadConfig(env: NodeJS.ProcessEnv, cwd: string): DaemonConfig {
  const rawPort = env.HELMSMAN_PORT?.trim()
  const port = rawPort ? Number(rawPort) : DEFAULT_PORT
  if (!Number.isInteger(port) || port < 0 || port > 65535) {
    throw new Error(`HELMSMAN_PORT must be a port number, got "${rawPort}"`)
  }

  const configuredToken = env.HELMSMAN_TOKEN?.trim()
  const dataDir = env.HELMSMAN_DATA_DIR?.trim() || join(cwd, '.helmsman')
  const webDir = env.HELMSMAN_WEB_DIR?.trim() || undefined

  return {
    // Only a container sets HELMSMAN_HOST, and only with its port published to loopback.
    host: env.HELMSMAN_HOST?.trim() || LOOPBACK_HOST,
    port,
    dataDir,
    repoDir: env.HELMSMAN_REPO_DIR?.trim() || cwd,
    worktreesDir: env.HELMSMAN_WORKTREES_DIR?.trim() || join(dataDir, 'worktrees'),
    webIndexPath:
      env.HELMSMAN_WEB_INDEX?.trim() ||
      (webDir === undefined ? undefined : join(webDir, 'index.html')),
    webDir,
    token: configuredToken || randomBytes(24).toString('base64url'),
    tokenWasGenerated: !configuredToken,
  }
}
