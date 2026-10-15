import { appendFileSync, mkdirSync, writeFileSync } from 'node:fs'
import { homedir } from 'node:os'
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
import { addProject, loadOrCreateDaemonToken } from './daemon-access.js'
import { daemonEnv } from './daemon-env.js'
import { readGhosttyFont } from './ghostty-config.js'
import { isDashboardUrl, loadingPageUrl, opensInBrowser } from './navigation.js'
import { findParts } from './paths.js'
import {
  autoUpdateOn,
  isGitRepository,
  loadSettings,
  saveSettings,
  startingRepoDir,
} from './settings.js'
import { checkReleaseSignature, runSmoke } from './smoke.js'
import {
  freePort,
  loginPathLogLine,
  loginShellPath,
  startupMarks,
  waitForHealth,
} from './startup.js'
import { Updater } from './updater.js'
import { windowOptions } from './window-options.js'

const marks = startupMarks()
marks.mark('module start')

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
// False while the window shows the loading page and waits for the daemon.
let daemonReady = false
let marksLogged = false
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

// Asks for a folder until a git repository is picked. Undefined: the user gave up.
async function pickRepository(): Promise<string | undefined> {
  for (;;) {
    const answer = await dialog.showOpenDialog({
      title: 'Add a project',
      message: 'Pick the top folder of a git repository.',
      buttonLabel: 'Add',
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

// onForked runs in the same tick as the fork, so the window opens while the daemon starts.
async function startDaemon(repoDir: string | undefined, onForked: () => void): Promise<void> {
  // Right after login the shell can take seconds, so the daemon starts meanwhile and gets the PATH
  // later (see packages/daemon/src/login-path.ts).
  const login = process.platform === 'win32' ? undefined : loginShellPath(process.env.SHELL)
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
  const place = {
    port,
    dataDir: join(userData, 'data'),
    repoDir,
    webDir: parts.webDir,
    binDir,
    loginPath: undefined,
  }
  daemonExited = false
  // While it starts, the caller reports a failure, so the exit handler stays quiet.
  let starting = true
  const child = utilityProcess.fork(parts.daemonMain, [], {
    env: {
      ...daemonEnv(process.env, daemonToken, place),
      ...(login === undefined ? {} : { NATAMUX_PATH_PENDING: '1' }),
    },
    // Not a project folder: the daemon may run with none.
    cwd: app.getPath('home'),
    stdio: 'pipe',
    serviceName: 'Natamux daemon',
  })
  daemon = child
  marks.mark('daemon forked')
  onForked()
  let ready = (): void => undefined
  const readyMessage = new Promise<void>((resolve) => {
    ready = resolve
  })
  child.on('message', (message: unknown) => {
    if (message !== 'ready') {
      return
    }
    marks.mark('daemon ready')
    ready()
    void login?.then((answer) => {
      const problem = loginPathLogLine(answer, process.env.PATH)
      if (problem !== undefined) {
        log(problem)
      }
      const loginPath = 'path' in answer ? answer.path : undefined
      const env = daemonEnv(process.env, daemonToken, { ...place, loginPath })
      if (!daemonExited) {
        child.postMessage({ path: env.PATH })
      }
    })
  })
  child.stdout?.on('data', (chunk: Buffer) => log(chunk.toString()))
  child.stderr?.on('data', (chunk: Buffer) => log(chunk.toString()))
  child.on('exit', (code) => {
    daemonExited = true
    log(`--- the daemon exited with code ${code}\n`)
    if (!quitConfirmed && !starting) {
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

  try {
    await waitForHealth(`${dashboardUrl}/health`, {
      timeoutMs: 30_000,
      exited: () => daemonExited,
      ready: readyMessage,
    })
  } finally {
    starting = false
  }
}

function createWindow(): void {
  const dark = nativeTheme.shouldUseDarkColors
  const origin = new URL(dashboardUrl).origin
  const created = new BrowserWindow(windowOptions(join(here, 'preload.cjs'), dark))
  window = created
  marks.mark('window created')
  // By origin, not by the exact loading page URL: Chromium may rewrite a data: URL.
  const onDashboard = (): boolean => created.webContents.getURL().startsWith(`${origin}/`)
  created.once('ready-to-show', () => {
    marks.mark('ready-to-show')
    created.show()
  })
  created.webContents.on('did-finish-load', () => {
    if (!marksLogged && onDashboard()) {
      marksLogged = true
      marks.mark('did-finish-load')
      log(marks.line())
    }
  })
  created.on('closed', () => {
    window = undefined
  })

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
    // A check for CI and the codespace: the dashboard loads signed in, a terminal and an agent work
    // (smoke.ts), then the app quits. The exit code tells: on Windows the app's output may not reach
    // the console.
    created.webContents.on('did-finish-load', async () => {
      // The loading page finishes first.
      if (!onDashboard()) {
        return
      }
      await new Promise((resolve) => setTimeout(resolve, 4000))
      const signedIn: unknown = await created.webContents.executeJavaScript(
        `document.querySelector('[aria-label="Daemon token"]') === null && document.querySelector('#root')?.childElementCount > 0`,
      )
      console.log(`natamux-desktop: ${created.webContents.getURL()} signed in: ${String(signedIn)}`)
      const shot = process.env.NATAMUX_DESKTOP_SMOKE_SHOT
      if (shot) {
        writeFileSync(shot, (await created.webContents.capturePage()).toPNG())
      }
      let passed = signedIn === true
      if (passed) {
        // Also in the daemon log, which CI reads on Windows.
        let result = 'natamux-desktop: smoke passed, the terminal and the agent work'
        try {
          await runSmoke(dashboardUrl, daemonToken, process.env, process.platform)
          result += `, ${checkReleaseSignature()}`
        } catch (error) {
          passed = false
          const message = error instanceof Error ? error.message : String(error)
          result = `natamux-desktop: smoke failed: ${message}`
        }
        console.log(result)
        log(`${result}\n`)
      }
      quitConfirmed = true
      daemon?.kill()
      app.exit(passed ? 0 : 1)
    })
  }
  if (daemonReady) {
    void created.loadURL(`${dashboardUrl}/`)
  } else {
    // The dashboard load cuts this one short when the daemon is quick, and that rejects.
    created.loadURL(loadingPageUrl(dark)).catch(() => undefined)
  }
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
    detail: `${action} stops the daemon, and with it every agent and shell. Tasks, worktrees and their changes stay, and the agents start again when Natamux does.`,
    buttons: [action, 'Cancel'],
    defaultId: 1,
    cancelId: 1,
  })
  return answer.response === 0
}

// The daemon tells the open dashboards, so the project shows up without a restart.
async function addProjectFromMenu(): Promise<void> {
  if (dashboardUrl === '' || daemonExited) {
    return
  }
  const dir = await pickRepository()
  if (dir === undefined) {
    return
  }
  const problem = await addProject(dashboardUrl, daemonToken, dir)
  if (problem !== undefined) {
    await dialog.showMessageBox({
      type: 'warning',
      message: `Natamux could not add ${dir}`,
      detail: problem,
    })
  }
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
        { label: 'Add Project…', accelerator: 'CmdOrCtrl+Shift+O', click: addProjectFromMenu },
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
    autoUpdate: autoUpdateOn(loadSettings(settingsFile)),
    saveAutoUpdate: (on) =>
      saveSettings(settingsFile, { ...loadSettings(settingsFile), autoUpdate: on }),
    confirmRestart: () => confirmStop('Restart'),
    quit: () => {
      quitConfirmed = true
      app.quit()
    },
    changed: buildMenu,
    parent: () => window,
  })
}

// The page reads the token once, as it loads. Only the dashboard may ask. Not a window argument:
// other local users can read a process's arguments.
ipcMain.on('natamux:daemon-token', (event) => {
  const from = event.senderFrame?.url ?? ''
  event.returnValue =
    dashboardUrl !== '' && isDashboardUrl(from, new URL(dashboardUrl).origin) ? daemonToken : ''
})

// The Browse button of the projects panel. Only the dashboard may open a dialog.
ipcMain.handle('natamux:pick-folder', (event) => {
  const from = event.senderFrame?.url ?? ''
  if (dashboardUrl === '' || !isDashboardUrl(from, new URL(dashboardUrl).origin)) {
    return undefined
  }
  return pickRepository()
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
      } else if (window === undefined && dashboardUrl !== '') {
        // Closing the last window on Windows or Linux asked to quit. Cancel keeps the app usable.
        createWindow()
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
    marks.mark('whenReady')
    updater = makeUpdater()
    buildMenu()
    try {
      await startDaemon(startingRepoDir(loadSettings(settingsFile)), createWindow)
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
    daemonReady = true
    void window?.loadURL(`${dashboardUrl}/`)
    updater?.start()
  })
}
