import { appendFileSync, mkdirSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import {
  app,
  BrowserWindow,
  dialog,
  ipcMain,
  Menu,
  type MenuItemConstructorOptions,
  nativeTheme,
  shell,
  type UtilityProcess,
  utilityProcess,
} from 'electron'
import { writeCliShim } from './cli-shim.js'
import { loadOrCreateDaemonToken } from './daemon-access.js'
import { daemonEnv } from './daemon-env.js'
import { isDashboardUrl, opensInBrowser } from './navigation.js'
import { findParts } from './paths.js'
import { isGitRepository, loadSettings, saveSettings } from './settings.js'
import { freePort, loginPathLogLine, loginShellPath, waitForHealth } from './startup.js'
import { Updater } from './updater.js'

// Before the paths are read: the user data folder is named after the app.
app.setName('Natamux')
const here = dirname(fileURLToPath(import.meta.url))
const userData = app.getPath('userData')
const settingsFile = join(userData, 'settings.json')
const logFile = join(userData, 'logs', 'daemon.log')
const PREFERRED_PORT = 4317

let daemon: UtilityProcess | undefined
let daemonExited = false
let daemonToken = ''
let dashboardUrl = ''
let window: BrowserWindow | undefined
let quitConfirmed = false
let updater: Updater | undefined
const lastLines: string[] = []

function log(text: string): void {
  for (const line of text.split('\n').filter((part) => part.trim() !== '')) {
    lastLines.push(line)
    if (lastLines.length > 40) {
      lastLines.shift()
    }
  }
  try {
    appendFileSync(logFile, text)
  } catch {
    // The log is a help, not a need.
  }
}

// Asks for the repository until a git repository is picked. Undefined: the user gave up.
async function pickRepository(message: string): Promise<string | undefined> {
  for (;;) {
    const answer = await dialog.showOpenDialog({
      title: 'Open a repository',
      message,
      buttonLabel: 'Open',
      properties: ['openDirectory'],
    })
    const dir = answer.filePaths[0]
    if (answer.canceled || dir === undefined) {
      return undefined
    }
    if (isGitRepository(dir)) {
      return dir
    }
    await dialog.showMessageBox({
      type: 'warning',
      message: `${dir} is not a git repository`,
      detail: 'Pick the top folder of a repository, the one that holds .git.',
    })
  }
}

async function startDaemon(repoDir: string): Promise<void> {
  // The packaged app keeps the built dashboard in its resources (electron-builder.yml).
  const parts = findParts({
    ...process.env,
    NATAMUX_WEB_DIR:
      process.env.NATAMUX_WEB_DIR ??
      (app.isPackaged ? join(process.resourcesPath, 'web') : undefined),
  })
  const binDir = join(userData, 'bin')
  writeCliShim(binDir, process.execPath, parts.cliMain, process.platform)
  const port = await freePort(PREFERRED_PORT)
  daemonToken = loadOrCreateDaemonToken(join(userData, 'daemon-token'))
  dashboardUrl = `http://127.0.0.1:${port}`

  mkdirSync(dirname(logFile), { recursive: true })
  log(`\n--- ${new Date().toISOString()} starting the daemon on port ${port}\n`)
  let loginPath: string | undefined
  if (process.platform !== 'win32') {
    const login = await loginShellPath(process.env.SHELL)
    loginPath = 'path' in login ? login.path : undefined
    const problem = loginPathLogLine(login, process.env.PATH)
    if (problem !== undefined) {
      log(problem)
    }
  }
  daemonExited = false
  daemon = utilityProcess.fork(parts.daemonMain, [], {
    env: daemonEnv(process.env, daemonToken, {
      port,
      dataDir: join(userData, 'data'),
      repoDir,
      webDir: parts.webDir,
      binDir,
      loginPath,
    }),
    cwd: repoDir,
    stdio: 'pipe',
    serviceName: 'Natamux daemon',
  })
  daemon.stdout?.on('data', (chunk: Buffer) => log(chunk.toString()))
  daemon.stderr?.on('data', (chunk: Buffer) => log(chunk.toString()))
  daemon.on('exit', (code) => {
    daemonExited = true
    log(`--- the daemon exited with code ${code}\n`)
    if (!quitConfirmed) {
      void dialog
        .showMessageBox({
          type: 'error',
          message: 'The Natamux daemon stopped',
          detail: `${lastLines.slice(-12).join('\n')}\n\nThe full log: ${logFile}`,
          buttons: ['Quit'],
        })
        .then(() => app.quit())
    }
  })

  await waitForHealth(`${dashboardUrl}/health`, {
    timeoutMs: 30_000,
    exited: () => daemonExited,
  })
}

function createWindow(): void {
  const created = new BrowserWindow({
    width: 1440,
    height: 900,
    minWidth: 800,
    minHeight: 560,
    show: false,
    title: 'Natamux',
    backgroundColor: nativeTheme.shouldUseDarkColors ? '#18181b' : '#ffffff',
    webPreferences: {
      preload: join(here, 'preload.cjs'),
      contextIsolation: true,
      sandbox: true,
      nodeIntegration: false,
      spellcheck: false,
    },
  })
  window = created
  created.once('ready-to-show', () => created.show())
  created.on('closed', () => {
    window = undefined
  })

  const origin = new URL(dashboardUrl).origin
  created.webContents.setWindowOpenHandler(({ url }) => {
    if (opensInBrowser(url)) {
      void shell.openExternal(url)
    }
    return { action: 'deny' }
  })
  created.webContents.on('will-navigate', (event, url) => {
    if (!isDashboardUrl(url, origin)) {
      event.preventDefault()
      if (opensInBrowser(url)) {
        void shell.openExternal(url)
      }
    }
  })
  if (process.env.NATAMUX_DESKTOP_SMOKE === '1') {
    // A check for CI and the codespace: the dashboard loads signed in, then the app quits. The
    // exit code tells: on Windows the app's output may not reach the console.
    created.webContents.once('did-finish-load', async () => {
      await new Promise((resolve) => setTimeout(resolve, 4000))
      const signedIn: unknown = await created.webContents.executeJavaScript(
        `document.querySelector('[aria-label="Daemon token"]') === null && document.querySelector('#root')?.childElementCount > 0`,
      )
      console.log(`natamux-desktop: ${created.webContents.getURL()} signed in: ${String(signedIn)}`)
      const shot = process.env.NATAMUX_DESKTOP_SMOKE_SHOT
      if (shot) {
        writeFileSync(shot, (await created.webContents.capturePage()).toPNG())
      }
      quitConfirmed = true
      daemon?.kill()
      app.exit(signedIn === true ? 0 : 1)
    })
  }
  void created.loadURL(`${dashboardUrl}/`)
}

// The number of tasks whose agent works now. Quitting stops them.
async function runningTasks(): Promise<number> {
  try {
    const response = await fetch(`${dashboardUrl}/tasks?state=running`, {
      headers: { authorization: `Bearer ${daemonToken}` },
    })
    const body = (await response.json()) as { tasks?: unknown[] }
    return body.tasks?.length ?? 0
  } catch {
    return 0
  }
}

async function confirmStop(action: string): Promise<boolean> {
  const running = await runningTasks()
  if (running === 0) {
    return true
  }
  const answer = await dialog.showMessageBox({
    type: 'warning',
    message: `${running === 1 ? 'An agent is' : `${running} agents are`} still working`,
    detail: `${action} stops the daemon, and with it every agent and shell. Tasks, worktrees and their changes stay.`,
    buttons: [action, 'Cancel'],
    defaultId: 1,
    cancelId: 1,
  })
  return answer.response === 0
}

async function openRepository(): Promise<void> {
  const dir = await pickRepository('The daemon starts again with this repository.')
  if (dir === undefined || !(await confirmStop('Restart'))) {
    return
  }
  saveSettings(settingsFile, { ...loadSettings(settingsFile), repoDir: dir })
  quitConfirmed = true
  app.relaunch()
  app.quit()
}

function buildMenu(): void {
  const mac = process.platform === 'darwin'
  const updates = updater?.menuItems() ?? []
  const template: MenuItemConstructorOptions[] = [
    ...(mac
      ? [
          {
            label: app.name,
            submenu: [
              { role: 'about' },
              ...(updates.length > 0 ? [{ type: 'separator' as const }, ...updates] : []),
              { type: 'separator' },
              { role: 'services' },
              { type: 'separator' },
              { role: 'hide' },
              { role: 'hideOthers' },
              { role: 'unhide' },
              { type: 'separator' },
              { role: 'quit' },
            ] satisfies MenuItemConstructorOptions[],
          },
        ]
      : []),
    {
      label: 'File',
      submenu: [
        { label: 'Open Repository…', accelerator: 'CmdOrCtrl+Shift+O', click: openRepository },
        { label: 'Show Daemon Log', click: () => void shell.openPath(logFile) },
        { type: 'separator' },
        mac ? { role: 'close' } : { role: 'quit' },
      ],
    },
    { role: 'editMenu' },
    { role: 'viewMenu' },
    { role: 'windowMenu' },
    ...(!mac && updates.length > 0 ? [{ label: 'Help', submenu: updates }] : []),
  ]
  Menu.setApplicationMenu(Menu.buildFromTemplate(template))
}

// Only a packaged app updates: a run from the repository has no release to replace.
function makeUpdater(): Updater | undefined {
  if (!app.isPackaged || process.env.NATAMUX_DESKTOP_SMOKE === '1') {
    return undefined
  }
  return new Updater({
    userData,
    version: process.env.NATAMUX_UPDATE_AS_VERSION || app.getVersion(),
    autoUpdate: loadSettings(settingsFile).autoUpdate !== false,
    saveAutoUpdate: (on) =>
      saveSettings(settingsFile, { ...loadSettings(settingsFile), autoUpdate: on }),
    confirmRestart: () => confirmStop('Restart'),
    quit: () => {
      quitConfirmed = true
      app.quit()
    },
    changed: buildMenu,
  })
}

// The page reads the token once, as it loads. Only the dashboard may ask.
ipcMain.on('natamux:daemon-token', (event) => {
  const from = event.senderFrame?.url ?? ''
  event.returnValue =
    dashboardUrl !== '' && isDashboardUrl(from, new URL(dashboardUrl).origin) ? daemonToken : ''
})

if (!app.requestSingleInstanceLock()) {
  app.quit()
} else {
  app.on('second-instance', () => {
    if (window?.isMinimized()) {
      window.restore()
    }
    window?.focus()
  })

  app.on('before-quit', (event) => {
    if (quitConfirmed || daemon === undefined || daemonExited) {
      return
    }
    event.preventDefault()
    void confirmStop('Quit').then((yes) => {
      if (yes) {
        quitConfirmed = true
        app.quit()
      }
    })
  })

  app.on('will-quit', () => {
    updater?.installOnQuit()
    daemon?.kill()
  })

  // On macOS the app, the daemon and the agents keep running with no window, as other Mac apps do.
  app.on('window-all-closed', () => {
    if (process.platform !== 'darwin') {
      app.quit()
    }
  })

  app.on('activate', () => {
    if (window === undefined && dashboardUrl !== '') {
      createWindow()
    }
  })

  void app.whenReady().then(async () => {
    updater = makeUpdater()
    buildMenu()
    let repoDir = loadSettings(settingsFile).repoDir
    if (repoDir === undefined || !isGitRepository(repoDir)) {
      repoDir = await pickRepository('Pick the repository whose tasks Natamux runs.')
      if (repoDir === undefined) {
        quitConfirmed = true
        app.quit()
        return
      }
      saveSettings(settingsFile, { ...loadSettings(settingsFile), repoDir })
    }
    try {
      await startDaemon(repoDir)
    } catch (error) {
      quitConfirmed = true
      await dialog.showMessageBox({
        type: 'error',
        message: 'Natamux could not start its daemon',
        detail: `${error instanceof Error ? error.message : String(error)}\n\n${lastLines.slice(-12).join('\n')}`,
      })
      app.quit()
      return
    }
    createWindow()
    updater?.start()
  })
}
