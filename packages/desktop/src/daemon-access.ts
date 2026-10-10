import { randomBytes } from 'node:crypto'
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { dirname } from 'node:path'

// The daemon's token, kept between runs so the `natamux` command and open pages keep working.
// Only the user may read the file.
export function loadOrCreateDaemonToken(file: string): string {
  try {
    const saved = readFileSync(file, 'utf8').trim()
    if (saved.length >= 32) {
      return saved
    }
  } catch {
    // None yet.
  }
  const created = randomBytes(24).toString('base64url')
  mkdirSync(dirname(file), { recursive: true })
  writeFileSync(file, `${created}\n`, { mode: 0o600 })
  return created
}

// Adds a project through the daemon, so the open dashboards show it at once. Returns why it
// failed, or undefined.
export async function addProject(
  dashboardUrl: string,
  token: string,
  path: string,
  post: typeof fetch = fetch,
): Promise<string | undefined> {
  try {
    const response = await post(`${dashboardUrl}/repos`, {
      method: 'POST',
      headers: { authorization: `Bearer ${token}`, 'content-type': 'application/json' },
      body: JSON.stringify({ path }),
    })
    if (response.ok) {
      return undefined
    }
    const body: unknown = await response.json().catch(() => undefined)
    const message =
      typeof body === 'object' && body !== null && 'message' in body ? body.message : undefined
    if (typeof message === 'string') {
      return message
    }
    return `The daemon answered with status ${response.status}`
  } catch (error) {
    return error instanceof Error ? error.message : String(error)
  }
}
