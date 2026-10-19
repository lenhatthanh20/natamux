import { createHash } from 'node:crypto'
import { chmodSync, createWriteStream, mkdirSync, readdirSync, renameSync, rmSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { Readable, Transform } from 'node:stream'
import { pipeline } from 'node:stream/promises'
import type { ReadableStream } from 'node:stream/web'
import { net } from 'electron'
import { unpackMac } from './mac-install.js'
import { type InstallKind, type Release, SUMS_FILE, sha256From } from './update-check.js'

// shortcut: below about 85 KB/s a 150 MB build cannot finish; use a stall timer if users hit it.
const DOWNLOAD_TIMEOUT_MS = 30 * 60_000

export type DownloadContext = {
  kind: InstallKind
  // The folder of the downloads: userData/updates.
  dir: string
  headers: Record<string, string>
  log: (text: string) => void
}

function entries(dir: string): string[] {
  try {
    return readdirSync(dir)
  } catch {
    return []
  }
}

// Streams a download to a file and returns its SHA-256.
async function save(
  context: DownloadContext,
  url: string,
  file: string,
  progress: (done: number, total: number | undefined) => void,
): Promise<string> {
  // The signal also ends the body stream, so a download stuck halfway fails the pipeline.
  const response = await net.fetch(url, {
    headers: context.headers,
    signal: AbortSignal.timeout(DOWNLOAD_TIMEOUT_MS),
  })
  if (!response.ok || response.body === null) {
    throw new Error(`Download of ${url} failed: HTTP ${response.status}`)
  }
  const length = Number(response.headers.get('content-length'))
  const total = Number.isFinite(length) && length > 0 ? length : undefined
  const hash = createHash('sha256')
  let received = 0
  await pipeline(
    Readable.fromWeb(response.body as unknown as ReadableStream),
    new Transform({
      transform(chunk: Buffer, _encoding, done) {
        hash.update(chunk)
        received += chunk.length
        progress(received, total)
        done(null, chunk)
      },
    }),
    createWriteStream(file),
  )
  return hash.digest('hex')
}

// sums: SHA256SUMS.txt of the release, already checked by fetchSignedSums.
export async function downloadRelease(
  context: DownloadContext,
  release: Release,
  name: string,
  sums: Buffer,
  progress: (done: number, total: number | undefined) => void,
): Promise<string> {
  const url = `${release.downloads}${name}`
  const expected = sha256From(sums.toString('utf8'), name)
  if (expected === undefined) {
    throw new Error(`${SUMS_FILE} has no line for ${name}`)
  }

  // Only the newest download is kept.
  for (const entry of entries(context.dir)) {
    if (entry !== release.version) {
      rmSync(join(context.dir, entry), { recursive: true, force: true })
    }
  }
  const folder = join(context.dir, release.version)
  mkdirSync(folder, { recursive: true })
  const appImage = process.env.APPIMAGE ?? ''
  // An AppImage downloads next to itself, so the rename below stays on one disk.
  const part =
    context.kind === 'appimage'
      ? join(dirname(appImage), `.Natamux-${release.version}.AppImage.part`)
      : join(folder, `${name}.part`)

  context.log(`download: ${url}`)
  const actual = await save(context, url, part, progress)
  if (actual !== expected) {
    rmSync(part, { force: true })
    throw new Error(`The SHA-256 of ${name} does not match ${SUMS_FILE}`)
  }
  context.log(`download: ${name} matches ${SUMS_FILE}`)

  if (context.kind === 'appimage') {
    chmodSync(part, 0o755)
    renameSync(part, appImage)
    return appImage
  }
  const file = join(folder, name)
  renameSync(part, file)
  return context.kind === 'mac' ? await unpackMac(file, folder, release.version) : file
}
