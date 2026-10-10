// Finds a new release, downloads it in the background and installs it on restart. See
// docs/design-desktop-update.md.
import { execFile, spawn } from 'node:child_process'
import { createHash } from 'node:crypto'
import {
  accessSync,
  appendFileSync,
  chmodSync,
  constants,
  createWriteStream,
  mkdirSync,
  readdirSync,
  renameSync,
  rmdirSync,
  rmSync,
  writeFileSync,
} from 'node:fs'
import { dirname, join } from 'node:path'
import { Readable, Transform } from 'node:stream'
import { pipeline } from 'node:stream/promises'
import type { ReadableStream } from 'node:stream/web'
import { promisify } from 'node:util'
import { app, dialog, type MenuItemConstructorOptions, net, shell } from 'electron'
import {
  assetName,
  type InstallKind,
  installKind,
  isNewer,
  LATEST_URL,
  MAC_INSTALL_SCRIPT,
  macBundle,
  type Release,
  releaseFromUrl,
  SUMS_FILE,
  sha256From,
} from './update-check.js'

const run = promisify(execFile)
const FIRST_CHECK_MS = 30_000
const CHECK_EVERY_MS = 6 * 60 * 60 * 1000

type State =
  | { name: 'idle' }
  | { name: 'checking' }
  | { name: 'downloading'; release: Release }
  // file: the new Natamux.app (mac), the AppImage in place, or the .deb.
  | { name: 'ready'; release: Release; file: string }
  // A new version this install cannot put in place itself.
  | { name: 'manual'; release: Release }

export type UpdaterOptions = {
  userData: string
  // The version to compare against: app.getVersion(), or NATAMUX_UPDATE_AS_VERSION in tests.
  version: string
  autoUpdate: boolean
  saveAutoUpdate: (on: boolean) => void
  // Asks when agents are working. False: the user said no.
  confirmRestart: () => Promise<boolean>
  // Quits without asking again.
  quit: () => void
  // The state changed: the menu shows it.
  changed: () => void
}

function canWrite(path: string): boolean {
  try {
    accessSync(path, constants.W_OK)
    return true
  } catch {
    return false
  }
}

function reason(error: unknown): string {
  return error instanceof Error ? error.message : String(error)
}

export class Updater {
  private state: State = { name: 'idle' }
  private timer: NodeJS.Timeout | undefined
  private auto: boolean
  private installing = false
  // Versions the user has seen a dialog for in this run: timed checks do not ask again.
  private readonly asked = new Set<string>()
  private readonly kind: InstallKind
  private readonly dir: string
  private readonly logFile: string
  private readonly headers: Record<string, string>

  constructor(private readonly options: UpdaterOptions) {
    this.auto = options.autoUpdate
    this.kind = installKind({
      platform: process.platform,
      execPath: process.execPath,
      env: process.env,
      canWrite,
    })
    this.dir = join(options.userData, 'updates')
    this.logFile = join(options.userData, 'logs', 'update.log')
    this.headers = { 'user-agent': `Natamux/${app.getVersion()}` }
  }

  start(): void {
    this.log(`start: version ${this.options.version}, install kind ${this.kind}`)
    if (this.auto) {
      this.schedule(FIRST_CHECK_MS)
    }
  }

  menuItems(): MenuItemConstructorOptions[] {
    const state = this.state
    const main: MenuItemConstructorOptions =
      state.name === 'ready'
        ? {
            label: `Restart to Update to ${state.release.version}`,
            click: () => void this.restart(),
          }
        : state.name === 'downloading'
          ? { label: `Downloading Update ${state.release.version}…`, enabled: false }
          : state.name === 'checking'
            ? { label: 'Checking for Updates…', enabled: false }
            : state.name === 'manual'
              ? {
                  label: `Download Natamux ${state.release.version}…`,
                  click: () => void shell.openExternal(state.release.page),
                }
              : { label: 'Check for Updates…', click: () => void this.check(true) }
    return [
      main,
      {
        label: 'Check for Updates Automatically',
        type: 'checkbox',
        checked: this.auto,
        click: (item) => this.setAuto(item.checked),
      },
    ]
  }

  // Called as the app quits: a Mac update that is ready goes in now, without a restart.
  installOnQuit(): void {
    if (this.state.name === 'ready' && this.kind === 'mac' && !this.installing) {
      this.log(`install: ${this.state.release.version} on quit`)
      this.startMacInstall(this.state.file, false)
    }
  }

  async check(manual: boolean): Promise<void> {
    const state = this.state
    if (state.name === 'checking' || state.name === 'downloading') {
      return
    }
    if (state.name === 'ready') {
      await this.offerRestart(state.release, manual)
      return
    }
    this.set({ name: 'checking' })
    try {
      const release = await this.latest()
      if (!isNewer(release.version, this.options.version)) {
        this.log(`check: ${release.version} is the latest`)
        this.set({ name: 'idle' })
        if (manual) {
          await dialog.showMessageBox({
            type: 'info',
            message: 'Natamux is up to date',
            detail: `Version ${app.getVersion()} is the latest.`,
          })
        }
        return
      }
      this.log(`check: ${release.version} found`)
      const name = assetName(this.kind, process.arch)
      if (name === undefined) {
        this.set({ name: 'manual', release })
        await this.offerPage(release, manual)
        return
      }
      this.set({ name: 'downloading', release })
      const file = await this.download(release, name)
      this.set({ name: 'ready', release, file })
      await this.offerRestart(release, manual)
    } catch (error) {
      this.log(`failed: ${reason(error)}`)
      this.set({ name: 'idle' })
      if (manual) {
        await dialog.showMessageBox({
          type: 'warning',
          message: 'Natamux could not update',
          detail: `${reason(error)}\n\nThe log: ${this.logFile}`,
        })
      }
    }
  }

  private setAuto(on: boolean): void {
    this.auto = on
    this.options.saveAutoUpdate(on)
    this.log(`timed checks ${on ? 'on' : 'off'}`)
    if (on) {
      this.schedule(FIRST_CHECK_MS)
    } else {
      clearTimeout(this.timer)
    }
    this.options.changed()
  }

  private schedule(ms: number): void {
    clearTimeout(this.timer)
    this.timer = setTimeout(() => {
      void this.check(false).finally(() => {
        if (this.auto) {
          this.schedule(CHECK_EVERY_MS)
        }
      })
    }, ms)
  }

  private set(state: State): void {
    this.state = state
    this.options.changed()
  }

  private log(text: string): void {
    try {
      mkdirSync(dirname(this.logFile), { recursive: true })
      appendFileSync(this.logFile, `${new Date().toISOString()} ${text}\n`)
    } catch {
      // The log is a help, not a need.
    }
  }

  private async latest(): Promise<Release> {
    let url = LATEST_URL
    // A moved repository adds a redirect to its new /releases/latest first.
    for (let hop = 0; hop < 5; hop++) {
      url = await this.redirectOf(url)
      const release = releaseFromUrl(url)
      if (release !== undefined) {
        return release
      }
      if (!url.endsWith('/releases/latest')) {
        throw new Error(`${url} is not a release with a version such as v1.2.3`)
      }
    }
    throw new Error(`Too many redirects from ${LATEST_URL}`)
  }

  // net.fetch leaves Response.url empty after a redirect, so the request stops at the redirect.
  private redirectOf(url: string): Promise<string> {
    return new Promise((resolve, reject) => {
      const request = net.request({ url, method: 'HEAD', redirect: 'manual' })
      for (const [name, value] of Object.entries(this.headers)) {
        request.setHeader(name, value)
      }
      request.on('redirect', (_status, _method, redirectUrl) => {
        resolve(redirectUrl)
        request.abort()
      })
      request.on('response', (response) => {
        reject(new Error(`GitHub answered ${response.statusCode} for ${url}, not a redirect`))
      })
      request.on('error', reject)
      request.end()
    })
  }

  private async download(release: Release, name: string): Promise<string> {
    const url = `${release.downloads}${name}`
    const sumsResponse = await net.fetch(`${release.downloads}${SUMS_FILE}`, {
      headers: this.headers,
    })
    if (!sumsResponse.ok) {
      throw new Error(`Download of ${SUMS_FILE} failed: HTTP ${sumsResponse.status}`)
    }
    const expected = sha256From(await sumsResponse.text(), name)
    if (expected === undefined) {
      throw new Error(`${SUMS_FILE} has no line for ${name}`)
    }

    // Only the newest download is kept.
    for (const entry of this.entries()) {
      if (entry !== release.version) {
        rmSync(join(this.dir, entry), { recursive: true, force: true })
      }
    }
    const folder = join(this.dir, release.version)
    mkdirSync(folder, { recursive: true })
    const appImage = process.env.APPIMAGE ?? ''
    // An AppImage downloads next to itself, so the rename below stays on one disk.
    const part =
      this.kind === 'appimage'
        ? join(dirname(appImage), `.Natamux-${release.version}.AppImage.part`)
        : join(folder, `${name}.part`)

    this.log(`download: ${url}`)
    const actual = await this.save(url, part)
    if (actual !== expected) {
      rmSync(part, { force: true })
      throw new Error(`The SHA-256 of ${name} does not match ${SUMS_FILE}`)
    }
    this.log(`download: ${name} matches ${SUMS_FILE}`)

    if (this.kind === 'appimage') {
      chmodSync(part, 0o755)
      renameSync(part, appImage)
      return appImage
    }
    const file = join(folder, name)
    renameSync(part, file)
    return this.kind === 'mac' ? await this.unpackMac(file, folder, release.version) : file
  }

  private entries(): string[] {
    try {
      return readdirSync(this.dir)
    } catch {
      return []
    }
  }

  // Streams a download to a file and returns its SHA-256.
  private async save(url: string, file: string): Promise<string> {
    const response = await net.fetch(url, { headers: this.headers })
    if (!response.ok || response.body === null) {
      throw new Error(`Download of ${url} failed: HTTP ${response.status}`)
    }
    const hash = createHash('sha256')
    await pipeline(
      Readable.fromWeb(response.body as unknown as ReadableStream),
      new Transform({
        transform(chunk: Buffer, _encoding, done) {
          hash.update(chunk)
          done(null, chunk)
        },
      }),
      createWriteStream(file),
    )
    return hash.digest('hex')
  }

  // Copies Natamux.app out of the .dmg and checks it.
  private async unpackMac(dmg: string, folder: string, version: string): Promise<string> {
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

  private async offerPage(release: Release, manual: boolean): Promise<void> {
    if (!manual && this.asked.has(release.version)) {
      return
    }
    this.asked.add(release.version)
    const answer = await dialog.showMessageBox({
      type: 'info',
      message: `Natamux ${release.version} is available`,
      detail: `You have ${app.getVersion()}. This copy of Natamux cannot update itself. Download the new version from the release page.`,
      buttons: ['Open the Release Page', 'Later'],
      defaultId: 0,
      cancelId: 1,
    })
    if (answer.response === 0) {
      void shell.openExternal(release.page)
    }
  }

  private async offerRestart(release: Release, manual: boolean): Promise<void> {
    if (!manual && this.asked.has(release.version)) {
      return
    }
    this.asked.add(release.version)
    const answer = await dialog.showMessageBox({
      type: 'info',
      message: `Natamux ${release.version} is ready`,
      detail: `You have ${app.getVersion()}. Restart to update.`,
      buttons: ['Restart Now', 'Later', 'What’s New'],
      defaultId: 0,
      cancelId: 1,
    })
    if (answer.response === 0) {
      await this.restart()
    } else if (answer.response === 2) {
      void shell.openExternal(release.page)
    }
  }

  private async restart(): Promise<void> {
    const state = this.state
    if (state.name !== 'ready' || !(await this.options.confirmRestart())) {
      return
    }
    this.log(`install: ${state.release.version}`)
    if (this.kind === 'mac') {
      this.startMacInstall(state.file, true)
    } else if (this.kind === 'appimage') {
      app.relaunch({ execPath: state.file, args: process.argv.slice(1) })
    } else if (this.kind === 'deb') {
      try {
        await run('pkexec', ['dpkg', '-i', state.file])
      } catch (error) {
        this.log(`install: pkexec failed: ${reason(error)}`)
        await dialog.showMessageBox({
          type: 'warning',
          message: 'Natamux could not install the update',
          detail: `Install it from a terminal, then start Natamux again:\n\nsudo apt install ${state.file}`,
        })
        return
      }
      app.relaunch()
    }
    this.options.quit()
  }

  // The script waits for this process to end, then swaps the bundles.
  private startMacInstall(unpacked: string, relaunch: boolean): void {
    const bundle = macBundle(process.execPath)
    if (bundle === undefined) {
      return
    }
    const script = join(dirname(unpacked), 'install.sh')
    writeFileSync(script, MAC_INSTALL_SCRIPT, { mode: 0o755 })
    spawn(
      '/bin/sh',
      [script, String(process.pid), unpacked, bundle, this.logFile, relaunch ? '1' : '0'],
      { detached: true, stdio: 'ignore' },
    ).unref()
    this.installing = true
  }
}
