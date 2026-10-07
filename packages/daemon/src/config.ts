import { randomBytes } from 'node:crypto'
import { join } from 'node:path'

export const DEFAULT_PORT = 4317
export const LOOPBACK_HOST = '127.0.0.1'

export type DaemonConfig = {
  host: string
  port: number
  dataDir: string
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

  return {
    host: LOOPBACK_HOST,
    port,
    dataDir: env.HELMSMAN_DATA_DIR?.trim() || join(cwd, '.helmsman'),
    token: configuredToken || randomBytes(24).toString('base64url'),
    tokenWasGenerated: !configuredToken,
  }
}
