import { execFile, spawn } from 'node:child_process'
import { mkdirSync, rmdirSync, rmSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { promisify } from 'node:util'
import { MAC_INSTALL_SCRIPT, macBundle } from './update-check.js'

const run = promisify(execFile)

// Copies Natamux.app out of the .dmg and checks it.
export async function unpackMac(dmg: string, folder: string, version: string): Promise<string> {
  const mount = join(folder, 'mnt')
  const unpacked = join(folder, 'Natamux.app')
  rmSync(unpacked, { recursive: true, force: true })
  mkdirSync(mount, { recursive: true })
  await run('hdiutil', [
    'attach',
    '-nobrowse',
    '-readonly',
    '-noautoopen',
    '-mountpoint',
    mount,
    dmg,
  ])
  try {
    await run('ditto', [join(mount, 'Natamux.app'), unpacked])
  } finally {
    await run('hdiutil', ['detach', mount, '-force']).catch(() => undefined)
    // rmdir: only an empty folder, never the disk image if it is still attached.
    try {
      rmdirSync(mount)
    } catch {
      // Left for the next download, which clears the folder.
    }
  }
  rmSync(dmg, { force: true })
  await run('codesign', ['--verify', '--deep', '--strict', unpacked])
  const { stdout } = await run('plutil', [
    '-extract',
    'CFBundleShortVersionString',
    'raw',
    '-o',
    '-',
    join(unpacked, 'Contents', 'Info.plist'),
  ])
  if (stdout.trim() !== version) {
    throw new Error(`The downloaded app is version ${stdout.trim()}, not ${version}`)
  }
  return unpacked
}

// The script waits for this process to end, then swaps the bundles. False: there is no bundle.
export function startMacInstall(unpacked: string, relaunch: boolean, logFile: string): boolean {
  const bundle = macBundle(process.execPath)
  if (bundle === undefined) {
    return false
  }
  const script = join(dirname(unpacked), 'install.sh')
  writeFileSync(script, MAC_INSTALL_SCRIPT, { mode: 0o755 })
  spawn('/bin/sh', [script, String(process.pid), unpacked, bundle, logFile, relaunch ? '1' : '0'], {
    detached: true,
    stdio: 'ignore',
  }).unref()
  return true
}
