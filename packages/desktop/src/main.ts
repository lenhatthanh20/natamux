import { homedir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { app, dialog, ipcMain, powerSaveBlocker } from 'electron'
import { AwakeGuard, readAgentStatus } from './app/agent-status.js'
import { readGhosttyFont } from './app/ghostty-config.js'
import { buildMenu } from './app/menu.js'
import { isDashboardUrl } from './app/navigation.js'
import { confirmStop } from './app/quit.js'
import { keepAwakeOn, loadSettings, startingRepoDir } from './app/settings.js'
import type { AppContext, AppState } from './app/state.js'
import { createWindow } from './app/window.js'
import { DAEMON_STOP_WAIT_MS, pickRepository, startDaemon } from './daemon/process.js'
import { startupMarks } from './daemon/startup.js'
import { makeUpdater } from './update/make-updater.js'

const marks = startupMarks()
marks.mark('module start')

// Before the paths are read: the user data folder is named after the app.
app.setName('Natamux')
const here = dirname(fileURLToPath(import.meta.url))
const userData = app.getPath('userData')
const settingsFile = join(userData, 'settings.json')

const awake = new AwakeGuard(powerSaveBlocker)
const state: AppState = {
  daemon: undefined,
  daemonExited: false,
  daemonToken: '',
  dashboardUrl: '',
  daemonReady: false,
  marksLogged: false,
  window: undefined,
  quitConfirmed: false,
  daemonStopWaited: false,
  updater: undefined,
  keepAwake: keepAwakeOn(loadSettings(settingsFile)),
  runningAgents: 0,
  lastLines: [],
}
const ctx: AppContext = {
  state,
  marks,
  awake,
  userData,
  settingsFile,
  logFile: join(userData, 'logs', 'daemon.log'),
  preloadFile: join(here, 'preload.cjs'),
}

// The page reads the token once, as it loads. Only the dashboard may ask. Not a window argument:
// other local users can read a process's arguments.
ipcMain.on('natamux:daemon-token', (event) => {
  const from = event.senderFrame?.url ?? ''
  event.returnValue =
    state.dashboardUrl !== '' && isDashboardUrl(from, new URL(state.dashboardUrl).origin)
      ? state.daemonToken
      : ''
})

// The Browse button of the projects panel. Only the dashboard may open a dialog.
ipcMain.handle('natamux:pick-folder', (event) => {
  const from = event.senderFrame?.url ?? ''
  if (state.dashboardUrl === '' || !isDashboardUrl(from, new URL(state.dashboardUrl).origin)) {
    return undefined
  }
  return pickRepository()
})

// The dashboard reports how many tasks need the user and how many run: the count shows on the app
// icon, and running agents keep the computer awake. Only the dashboard may report.
ipcMain.on('natamux:status', (event, value: unknown) => {
  const from = event.senderFrame?.url ?? ''
  const status = readAgentStatus(value)
  if (
    status === undefined ||
    state.dashboardUrl === '' ||
    !isDashboardUrl(from, new URL(state.dashboardUrl).origin)
  ) {
    return
  }
  app.setBadgeCount(status.needsYou)
  state.runningAgents = status.running
  awake.update(state.runningAgents > 0 && state.keepAwake)
})

// The terminals of the dashboard use Ghostty's font when the user has one and picked none.
// Read again at each load, so a change in Ghostty shows after a reload.
ipcMain.on('natamux:ghostty-font', (event) => {
  const font = readGhosttyFont(process.env, homedir(), process.platform)
  event.returnValue = font === undefined ? '' : JSON.stringify(font)
})

if (!app.requestSingleInstanceLock()) {
  app.quit()
} else {
  app.on('second-instance', () => {
    if (state.window?.isMinimized()) {
      state.window.restore()
    }
    state.window?.focus()
  })

  app.on('before-quit', (event) => {
    if (state.quitConfirmed || state.daemon === undefined || state.daemonExited) {
      return
    }
    event.preventDefault()
    void confirmStop(state, 'Quit').then((yes) => {
      if (yes) {
        state.quitConfirmed = true
        app.quit()
      } else if (state.window === undefined && state.dashboardUrl !== '') {
        // Closing the last window on Windows or Linux asked to quit. Cancel keeps the app usable.
        createWindow(ctx)
      }
    })
  })

  // The daemon waits up to 3 s for its shells to end. The app waits for it: its exit would cut the
  // daemon short and leave a shell that ignores SIGHUP running.
  app.on('will-quit', (event) => {
    const daemon = state.daemon
    if (daemon !== undefined && !state.daemonExited && !state.daemonStopWaited) {
      event.preventDefault()
      state.daemonStopWaited = true
      const timer = setTimeout(() => app.quit(), DAEMON_STOP_WAIT_MS)
      daemon.once('exit', () => {
        clearTimeout(timer)
        app.quit()
      })
      daemon.kill()
      return
    }
    state.updater?.installOnQuit()
  })

  // On macOS the app, the daemon and the agents keep running with no window, as other Mac apps do.
  app.on('window-all-closed', () => {
    if (process.platform !== 'darwin') {
      app.quit()
    }
  })

  app.on('activate', () => {
    if (state.window === undefined && state.dashboardUrl !== '') {
      createWindow(ctx)
    }
  })

  void app.whenReady().then(async () => {
    marks.mark('whenReady')
    state.updater = makeUpdater(ctx)
    buildMenu(ctx)
    try {
      await startDaemon(ctx, startingRepoDir(loadSettings(settingsFile)), () => createWindow(ctx))
    } catch (error) {
      state.quitConfirmed = true
      await dialog.showMessageBox({
        type: 'error',
        message: 'Natamux could not start its daemon',
        detail: `${error instanceof Error ? error.message : String(error)}\n\n${state.lastLines.slice(-12).join('\n')}`,
      })
      app.quit()
      return
    }
    state.daemonReady = true
    void state.window?.loadURL(`${state.dashboardUrl}/`)
    state.updater?.start()
  })
}
