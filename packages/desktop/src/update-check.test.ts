import { execFileSync } from 'node:child_process'
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import {
  assetName,
  installKind,
  isNewer,
  MAC_INSTALL_SCRIPT,
  macBundle,
  parseVersion,
  releaseFromUrl,
  sha256From,
} from './update-check.js'

let dir: string

beforeEach(() => {
  dir = mkdtempSync(join(tmpdir(), 'natamux-update-'))
})

afterEach(() => {
  rmSync(dir, { recursive: true, force: true })
})

describe('versions', () => {
  it('reads MAJOR.MINOR.PATCH with an optional v, and nothing else', () => {
    expect(parseVersion('v0.1.2')).toEqual([0, 1, 2])
    expect(parseVersion('10.0.31')).toEqual([10, 0, 31])
    expect(parseVersion('v1.2.0-beta.1')).toBeUndefined()
    expect(parseVersion('1.2')).toBeUndefined()
  })

  it('compares part by part, as numbers', () => {
    expect(isNewer('0.1.2', '0.1.1')).toBe(true)
    expect(isNewer('0.10.0', '0.9.9')).toBe(true)
    expect(isNewer('1.0.0', '0.99.99')).toBe(true)
    expect(isNewer('0.1.1', '0.1.1')).toBe(false)
    expect(isNewer('0.1.0', '0.1.1')).toBe(false)
    expect(isNewer('0.2.0-beta.1', '0.1.1')).toBe(false)
  })
})

describe('releaseFromUrl', () => {
  const repo = 'https://github.com/natamux/natamux'

  it('reads the version, the page and the download folder of a stable release', () => {
    expect(releaseFromUrl(`${repo}/releases/tag/v0.1.2`)).toEqual({
      version: '0.1.2',
      page: `${repo}/releases/tag/v0.1.2`,
      downloads: `${repo}/releases/download/v0.1.2/`,
    })
  })

  it('drops a pre-release tag, the release list and other sites', () => {
    expect(releaseFromUrl(`${repo}/releases/tag/v0.2.0-rc.1`)).toBeUndefined()
    expect(releaseFromUrl(`${repo}/releases`)).toBeUndefined()
    expect(releaseFromUrl('https://example.test/a/b/releases/tag/v1.0.0')).toBeUndefined()
  })
})

describe('installKind', () => {
  const app = '/Applications/Natamux.app/Contents/MacOS/Natamux'
  const anywhere = () => true

  it('finds the bundle of the running Mac app', () => {
    expect(macBundle(app)).toBe('/Applications/Natamux.app')
    expect(macBundle('/usr/bin/node')).toBeUndefined()
  })

  it('replaces a Mac app only from a folder it can write', () => {
    const mac = { platform: 'darwin' as const, env: {} }
    expect(installKind({ ...mac, execPath: app, canWrite: anywhere })).toBe('mac')
    expect(installKind({ ...mac, execPath: app, canWrite: () => false })).toBe('manual')
    expect(
      installKind({
        ...mac,
        execPath: '/Volumes/Natamux/Natamux.app/Contents/MacOS/Natamux',
        canWrite: anywhere,
      }),
    ).toBe('manual')
    expect(
      installKind({
        ...mac,
        execPath: '/private/var/folders/x/AppTranslocation/y/d/Natamux.app/Contents/MacOS/Natamux',
        canWrite: anywhere,
      }),
    ).toBe('manual')
  })

  it('tells the AppImage, the .deb and the .tar.gz apart on Linux', () => {
    const linux = { platform: 'linux' as const, canWrite: anywhere }
    expect(
      installKind({
        ...linux,
        execPath: '/tmp/.mount_x/natamux-desktop',
        env: { APPIMAGE: '/home/me/Apps/Natamux.AppImage' },
      }),
    ).toBe('appimage')
    expect(
      installKind({
        ...linux,
        execPath: '/tmp/.mount_x/natamux-desktop',
        env: { APPIMAGE: '/opt/apps/Natamux.AppImage' },
        canWrite: (path) => path !== '/opt/apps',
      }),
    ).toBe('manual')
    expect(installKind({ ...linux, execPath: '/opt/Natamux/natamux-desktop', env: {} })).toBe('deb')
    const tarball = '/home/me/natamux/natamux-desktop'
    expect(installKind({ ...linux, execPath: tarball, env: {} })).toBe('manual')
  })

  it('only opens the release page on Windows', () => {
    expect(
      installKind({ platform: 'win32', execPath: 'C:\\Natamux.exe', env: {}, canWrite: anywhere }),
    ).toBe('manual')
  })
})

describe('assetName', () => {
  it('names the release file of each install kind and machine', () => {
    expect(assetName('mac', 'arm64')).toBe('Natamux-mac-arm64.dmg')
    expect(assetName('mac', 'x64')).toBe('Natamux-mac-x64.dmg')
    expect(assetName('appimage', 'x64')).toBe('Natamux-linux-x86_64.AppImage')
    expect(assetName('deb', 'x64')).toBe('Natamux-linux-amd64.deb')
    expect(assetName('deb', 'arm64')).toBeUndefined()
    expect(assetName('manual', 'x64')).toBeUndefined()
  })
})

describe('sha256From', () => {
  const hash = 'A'.repeat(64)
  const sums = `${'0'.repeat(64)}  Natamux-linux-amd64.deb\n${hash} *Natamux-mac-arm64.dmg\n`

  it('finds the line of one file, in text or binary form', () => {
    expect(sha256From(sums, 'Natamux-mac-arm64.dmg')).toBe('a'.repeat(64))
    expect(sha256From(sums, 'Natamux-linux-amd64.deb')).toBe('0'.repeat(64))
    expect(sha256From(sums, 'Natamux-mac-x64.dmg')).toBeUndefined()
  })
})

describe.skipIf(process.platform === 'win32')('the Mac install script', () => {
  function bundle(path: string, text: string): void {
    mkdirSync(join(path, 'Contents'), { recursive: true })
    writeFileSync(join(path, 'Contents', 'version'), text)
  }

  function install(newApp: string, appPath: string, log: string): void {
    const script = join(dir, 'install.sh')
    writeFileSync(script, MAC_INSTALL_SCRIPT)
    // A pid that is gone: the script goes on at once.
    const gone = execFileSync('sh', ['-c', 'sh -c "echo \\$\\$"']).toString().trim()
    execFileSync('sh', [script, gone, newApp, appPath, log, '0'])
  }

  it('swaps the new bundle in and writes the log', () => {
    const appPath = join(dir, 'Applications', 'Natamux.app')
    const newApp = join(dir, 'updates', 'Natamux.app')
    const log = join(dir, 'update.log')
    bundle(appPath, 'old')
    bundle(newApp, 'new')
    install(newApp, appPath, log)
    expect(readFileSync(join(appPath, 'Contents', 'version'), 'utf8')).toBe('new')
    expect(existsSync(`${appPath}.old`)).toBe(false)
    expect(existsSync(newApp)).toBe(false)
    expect(readFileSync(log, 'utf8')).toMatch(/install: done/)
  })

  it('keeps the old bundle when the new one cannot move in', () => {
    const appPath = join(dir, 'Applications', 'Natamux.app')
    const log = join(dir, 'update.log')
    bundle(appPath, 'old')
    install(join(dir, 'missing', 'Natamux.app'), appPath, log)
    expect(readFileSync(join(appPath, 'Contents', 'version'), 'utf8')).toBe('old')
    expect(readFileSync(log, 'utf8')).toMatch(/kept the old one/)
  })
})
