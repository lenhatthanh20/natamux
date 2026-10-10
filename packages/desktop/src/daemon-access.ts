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
