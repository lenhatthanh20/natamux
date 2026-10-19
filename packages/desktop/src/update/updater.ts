// Finds a new release, downloads it in the background and installs it on restart. See
// docs/design-desktop-update.md.
import { execFile } from 'node:child_process'
import { accessSync, appendFileSync, constants, mkdirSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { promisify } from 'node:util'
import {
  app,
  type BrowserWindow,
  dialog,
  type MenuItemConstructorOptions,
  net,
  shell,
} from 'electron'
import { fetchSignedSums } from '../release-signature.js'
import { downloadRelease } from './download.js'
import { startMacInstall } from './mac-install.js'
import { PopupController } from './popup-controller.js'
import {
  assetName,
  type InstallKind,
  installKind,
  isNewer,
  LATEST_URL,
  type Release,
  releaseFromUrl,
  SUMS_FILE,
} from './update-check.js'
import { downloadingView } from './update-popup.js'

const run = promisify(execFile)
const FIRST_CHECK_MS = 30_000
const CHECK_EVERY_MS = 6 * 60 * 60 * 1000
// A hung request would keep the state at checking or downloading, and stop every later check.
const FETCH_TIMEOUT_MS = 30_000

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
  // The popup of a manual check sits over this window.
  parent: () => BrowserWindow | undefined
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
  private readonly popup: PopupController

  constructor(private readonly options: UpdaterOptions) {
    this.auto = options.autoUpdate
    this.popup = new PopupController(() => options.parent())
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
      this.installing = startMacInstall(this.state.file, false, this.logFile)
    }
  }

  async check(manual: boolean): Promise<void> {
    const state = this.state
    if (state.name === 'checking' || state.name === 'downloading') {
      return
    }
    if (manual) {
      this.popup.open()
    }
    const ready = state.name === 'ready' ? state : undefined
    if (ready !== undefined && !(await this.newerThanReady(ready.release))) {
      await this.offerRestart(ready.release, manual)
      return
    }
    this.set({ name: 'checking' })
    try {
      const release = await this.latest()
      if (!isNewer(release.version, this.options.version)) {
        this.log(`check: ${release.version} is the latest`)
        this.set({ name: 'idle' })
        await this.popup.close()
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
      // Before anything is offered, even a link to the release page.
      const sums = await fetchSignedSums({
        release,
        current: this.options.version,
        fetch: (url) =>
          net.fetch(url, { headers: this.headers, signal: AbortSignal.timeout(FETCH_TIMEOUT_MS) }),
      })
      this.log(`check: ${SUMS_FILE} is signed for natamux ${release.tag}`)
      const name = assetName(this.kind, process.arch)
      if (name === undefined) {
        this.set({ name: 'manual', release })
        await this.offerPage(release, manual)
        return
      }
      this.set({ name: 'downloading', release })
      this.popup.update(downloadingView(release.version, 0, undefined))
      const file = await downloadRelease(
        { kind: this.kind, dir: this.dir, headers: this.headers, log: (text) => this.log(text) },
        release,
        name,
        sums,
        (done, total) => this.popup.update(downloadingView(release.version, done, total)),
      )
      this.log(`ready: ${release.version} at ${file}`)
      this.set({ name: 'ready', release, file })
      await this.offerRestart(release, manual)
    } catch (error) {
      this.log(`failed: ${reason(error)}`)
      // A newer download that fails must not lose the update that is ready.
      this.set(ready ?? { name: 'idle' })
      await this.popup.close()
      if (manual) {
        await dialog.showMessageBox({
          type: 'warning',
          message: 'Natamux could not update',
          detail: `${reason(error)}\n\nThe log: ${this.logFile}`,
        })
      }
    }
  }

  private async newerThanReady(ready: Release): Promise<boolean> {
    try {
      return isNewer((await this.latest()).version, ready.version)
    } catch (error) {
      this.log(`check: ${reason(error)}`)
      return false
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
      // net.request takes no AbortSignal.
      const timer = setTimeout(() => {
        reject(new Error(`No answer from ${url} in ${FETCH_TIMEOUT_MS / 1000} seconds`))
        request.abort()
      }, FETCH_TIMEOUT_MS)
      request.on('redirect', (_status, _method, redirectUrl) => {
        clearTimeout(timer)
        resolve(redirectUrl)
        request.abort()
      })
      request.on('response', (response) => {
        clearTimeout(timer)
        reject(new Error(`GitHub answered ${response.statusCode} for ${url}, not a redirect`))
      })
      request.on('error', (error) => {
        clearTimeout(timer)
        reject(error)
      })
      request.end()
    })
  }

  private async offerPage(release: Release, manual: boolean): Promise<void> {
    await this.popup.close()
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
    await this.popup.close()
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
      this.installing = startMacInstall(state.file, true, this.logFile)
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
}
