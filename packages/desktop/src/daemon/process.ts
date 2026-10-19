import { appendFileSync, mkdirSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { app, dialog, utilityProcess } from 'electron'
import { isGitRepository } from '../app/settings.js'
import type { AppContext } from '../app/state.js'
import { writeCliShim } from './cli-shim.js'
import { loadOrCreateDaemonToken } from './daemon-access.js'
import { daemonEnv } from './daemon-env.js'
import { findParts } from './paths.js'
import { freePort, loginPathLogLine, loginShellPath, waitForHealth } from './startup.js'

const PREFERRED_PORT = 4317
// A daemon that does not exit must not keep the app from quitting.
export const DAEMON_STOP_WAIT_MS = 5000

export function log(ctx: AppContext, text: string): void {
  const { lastLines } = ctx.state
  for (const line of text.split('\n').filter((part) => part.trim() !== '')) {
    lastLines.push(line)
    if (lastLines.length > 40) {
      lastLines.shift()
    }
  }
  try {
    appendFileSync(ctx.logFile, text)
  } catch {
    // The log is a help, not a need.
  }
}

// Asks for a folder until a git repository is picked. Undefined: the user gave up.
export async function pickRepository(): Promise<string | undefined> {
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
export async function startDaemon(
  ctx: AppContext,
  repoDir: string | undefined,
  onForked: () => void,
): Promise<void> {
  const { state, marks, userData, logFile } = ctx
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
  state.daemonToken = loadOrCreateDaemonToken(join(userData, 'daemon-token'))
  state.dashboardUrl = `http://127.0.0.1:${port}`

  mkdirSync(dirname(logFile), { recursive: true })
  log(ctx, `\n--- ${new Date().toISOString()} starting the daemon on port ${port}\n`)
  const place = {
    port,
    dataDir: join(userData, 'data'),
    repoDir,
    webDir: parts.webDir,
    binDir,
    loginPath: undefined,
  }
  state.daemonExited = false
  // While it starts, the caller reports a failure, so the exit handler stays quiet.
  let starting = true
  const child = utilityProcess.fork(parts.daemonMain, [], {
    env: {
      ...daemonEnv(process.env, state.daemonToken, place),
      ...(login === undefined ? {} : { NATAMUX_PATH_PENDING: '1' }),
    },
    // Not a project folder: the daemon may run with none.
    cwd: app.getPath('home'),
    stdio: 'pipe',
    serviceName: 'Natamux daemon',
  })
  state.daemon = child
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
        log(ctx, problem)
      }
      const loginPath = 'path' in answer ? answer.path : undefined
      const env = daemonEnv(process.env, state.daemonToken, { ...place, loginPath })
      if (!state.daemonExited) {
        child.postMessage({ path: env.PATH })
      }
    })
  })
  child.stdout?.on('data', (chunk: Buffer) => log(ctx, chunk.toString()))
  child.stderr?.on('data', (chunk: Buffer) => log(ctx, chunk.toString()))
  child.on('exit', (code) => {
    state.daemonExited = true
    log(ctx, `--- the daemon exited with code ${code}\n`)
    if (!state.quitConfirmed && !starting) {
      void dialog
        .showMessageBox({
          type: 'error',
          message: 'The Natamux daemon stopped',
          detail: `${state.lastLines.slice(-12).join('\n')}\n\nThe full log: ${logFile}`,
          buttons: ['Quit'],
        })
        .then(() => app.quit())
    }
  })

  try {
    await waitForHealth(`${state.dashboardUrl}/health`, {
      timeoutMs: 30_000,
      exited: () => state.daemonExited,
      ready: readyMessage,
    })
  } finally {
    starting = false
  }
}
