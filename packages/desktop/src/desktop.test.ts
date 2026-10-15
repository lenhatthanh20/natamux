import type { execFile } from 'node:child_process'
import { mkdtempSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs'
import { createServer } from 'node:net'
import { tmpdir } from 'node:os'
import { delimiter, join } from 'node:path'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { writeCliShim } from './cli-shim.js'
import { addProject, loadOrCreateDaemonToken } from './daemon-access.js'
import { daemonEnv, fallbackBinDirs } from './daemon-env.js'
import { isDashboardUrl, isLoadingPage, loadingPageUrl, opensInBrowser } from './navigation.js'
import {
  autoUpdateOn,
  isGitRepository,
  loadSettings,
  saveSettings,
  startingRepoDir,
} from './settings.js'
import { hasSmokeOutput, smokeCommandLine } from './smoke.js'
import {
  freePort,
  loginPathLogLine,
  loginShellPath,
  startupMarks,
  waitForHealth,
} from './startup.js'
import { windowOptions } from './window-options.js'

type Done = (error: Error | null, stdout: string, stderr: string) => void

let dir: string

beforeEach(() => {
  dir = mkdtempSync(join(tmpdir(), 'natamux-desktop-'))
})

afterEach(() => {
  rmSync(dir, { recursive: true, force: true })
})

describe('the daemon token', () => {
  it('is made once, kept for the user only, and read back', () => {
    const file = join(dir, 'app', 'daemon-token')
    const first = loadOrCreateDaemonToken(file)
    expect(first.length).toBeGreaterThanOrEqual(32)
    expect(loadOrCreateDaemonToken(file)).toBe(first)
    if (process.platform !== 'win32') {
      expect(statSync(file).mode & 0o777).toBe(0o600)
    }
  })

  it('is made again when the saved one is too short to trust', () => {
    const file = join(dir, 'daemon-token')
    writeFileSync(file, 'short\n')
    expect(loadOrCreateDaemonToken(file)).not.toBe('short')
  })
})

describe('adding a project', () => {
  it('posts the folder to the daemon with the token', async () => {
    const calls: { url: string; init: RequestInit | undefined }[] = []
    const fake = (async (url: string, init?: RequestInit) => {
      calls.push({ url, init })
      return new Response(JSON.stringify({ repo: { id: 'web' } }), { status: 201 })
    }) as typeof fetch

    const problem = await addProject('http://127.0.0.1:4318', 'secret', '/work/web', fake)

    expect(problem).toBeUndefined()
    expect(calls).toHaveLength(1)
    expect(calls[0]?.url).toBe('http://127.0.0.1:4318/repos')
    expect(calls[0]?.init?.method).toBe('POST')
    expect(new Headers(calls[0]?.init?.headers).get('authorization')).toBe('Bearer secret')
    expect(JSON.parse(String(calls[0]?.init?.body))).toEqual({ path: '/work/web' })
  })

  it('tells why the daemon refused the folder, or could not be reached', async () => {
    const body = JSON.stringify({ error: 'invalid_request', message: '/w is added already' })
    const refused = (async () => new Response(body, { status: 400 })) as typeof fetch
    expect(await addProject('http://x', 'secret', '/w', refused)).toBe('/w is added already')

    const broken = (async () => new Response('oops', { status: 500 })) as typeof fetch
    expect(await addProject('http://x', 'secret', '/w', broken)).toBe(
      'The daemon answered with status 500',
    )

    const down = (async () => {
      throw new Error('connect ECONNREFUSED')
    }) as typeof fetch
    expect(await addProject('http://x', 'secret', '/w', down)).toBe('connect ECONNREFUSED')
  })
})

describe('settings', () => {
  it('keeps the repository, and starts over from a broken file', () => {
    const file = join(dir, 'settings.json')
    expect(loadSettings(file)).toEqual({})
    saveSettings(file, { repoDir: '/work/repo' })
    expect(loadSettings(file)).toEqual({ repoDir: '/work/repo' })
    writeFileSync(file, '{ not json')
    expect(loadSettings(file)).toEqual({})
  })

  it('keeps the update choice, and drops a value of the wrong type', () => {
    const file = join(dir, 'settings.json')
    saveSettings(file, { repoDir: '/work/repo', autoUpdate: false })
    expect(loadSettings(file)).toEqual({ repoDir: '/work/repo', autoUpdate: false })
    writeFileSync(file, '{"autoUpdate":"no"}')
    expect(loadSettings(file)).toEqual({})
  })

  it('checks for updates on its own only when the user turned it on', () => {
    const file = join(dir, 'settings.json')
    expect(autoUpdateOn(loadSettings(file))).toBe(false)
    writeFileSync(file, '{"autoUpdate":"yes"}')
    expect(autoUpdateOn(loadSettings(file))).toBe(false)
    saveSettings(file, { autoUpdate: false })
    expect(autoUpdateOn(loadSettings(file))).toBe(false)
    saveSettings(file, { autoUpdate: true })
    expect(autoUpdateOn(loadSettings(file))).toBe(true)
  })

  it('knows a git repository by its .git', () => {
    expect(isGitRepository(dir)).toBe(false)
    writeFileSync(join(dir, '.git'), 'gitdir: /elsewhere\n')
    expect(isGitRepository(dir)).toBe(true)
  })

  it('starts the daemon with the repository of an older version only while it is still one', () => {
    expect(startingRepoDir({})).toBeUndefined()
    expect(startingRepoDir({ repoDir: dir })).toBeUndefined()
    expect(startingRepoDir({ repoDir: join(dir, 'gone') })).toBeUndefined()
    const file = join(dir, 'settings.json')
    writeFileSync(file, JSON.stringify({ repoDir: '', autoUpdate: true }))
    expect(startingRepoDir(loadSettings(file))).toBeUndefined()
    writeFileSync(join(dir, '.git'), 'gitdir: /elsewhere\n')
    expect(startingRepoDir({ repoDir: dir })).toBe(dir)
  })
})

describe('daemonEnv', () => {
  const place = {
    port: 4318,
    dataDir: '/data',
    repoDir: '/repo',
    webDir: '/web',
    binDir: '/app/bin',
    loginPath: `/opt/homebrew/bin${delimiter}/usr/bin`,
  }

  it('gives the daemon its place, and the shells the natamux command first on the PATH', () => {
    const env = daemonEnv({ PATH: '/usr/bin', HOME: '/home/me' }, 'secret', place)
    expect(env).toMatchObject({
      HOME: '/home/me',
      NATAMUX_HOST: '127.0.0.1',
      NATAMUX_PORT: '4318',
      NATAMUX_TOKEN: 'secret',
      NATAMUX_DATA_DIR: '/data',
      NATAMUX_REPO_DIR: '/repo',
      NATAMUX_WEB_DIR: '/web',
      NATAMUX_BIN_DIR: '/app/bin',
      PATH: ['/app/bin', '/opt/homebrew/bin', '/usr/bin'].join(delimiter),
    })
  })

  it('gives the daemon no repository when there is none', () => {
    const env = daemonEnv({ NATAMUX_REPO_DIR: '/from/the/shell' }, 'secret', {
      ...place,
      repoDir: undefined,
    })
    expect('NATAMUX_REPO_DIR' in env).toBe(false)
  })

  it('keeps the app PATH when the login shell gave none, and drops ELECTRON_RUN_AS_NODE', () => {
    const noLogin = { ...place, loginPath: undefined }
    const env = daemonEnv({ PATH: '/usr/bin', ELECTRON_RUN_AS_NODE: '1' }, 'secret', noLogin, [])
    expect(env.PATH).toBe(['/app/bin', '/usr/bin'].join(delimiter))
    expect('ELECTRON_RUN_AS_NODE' in env).toBe(false)
  })

  it('adds the common install folders when the login shell gave no PATH', () => {
    const noLogin = { ...place, loginPath: undefined }
    const env = daemonEnv({ PATH: '/usr/bin' }, 'secret', noLogin, [
      '/opt/homebrew/bin',
      '/usr/bin',
    ])
    expect(env.PATH).toBe(['/app/bin', '/usr/bin', '/opt/homebrew/bin'].join(delimiter))
  })

  it('puts the natamux command first, then the login PATH once, with no fallback folders', () => {
    const loginPath = ['/opt/homebrew/bin', '/app/bin', '/opt/homebrew/bin'].join(delimiter)
    const repeated = { ...place, loginPath }
    expect(daemonEnv({ PATH: '/usr/bin' }, 'secret', repeated, ['/usr/local/bin']).PATH).toBe(
      ['/app/bin', '/opt/homebrew/bin'].join(delimiter),
    )
  })

  it('knows the common install folders that exist, on macOS and Linux only', () => {
    const exists = (folder: string) => folder !== '/opt/homebrew/sbin'
    expect(fallbackBinDirs('darwin', exists)).toEqual(['/opt/homebrew/bin', '/usr/local/bin'])
    expect(fallbackBinDirs('linux', () => false)).toEqual([])
    expect(fallbackBinDirs('win32', exists)).toEqual([])
  })
})

describe('startup', () => {
  it('takes the preferred port when free, and another one when it is taken', async () => {
    const taken = createServer()
    await new Promise<void>((resolve) => taken.listen(0, '127.0.0.1', resolve))
    const address = taken.address()
    const busy = typeof address === 'object' && address !== null ? address.port : 0
    try {
      const port = await freePort(busy)
      expect(port).not.toBe(busy)
      expect(port).toBeGreaterThan(0)
    } finally {
      taken.close()
    }
  })

  it('waits until the daemon answers', async () => {
    let calls = 0
    const fake = (async () => {
      calls += 1
      if (calls < 3) {
        throw new Error('refused')
      }
      return new Response('{}', { status: 200 })
    }) as typeof fetch
    await waitForHealth('http://x/health', {
      timeoutMs: 2000,
      exited: () => false,
      fetch: fake,
      stepMs: 1,
    })
    expect(calls).toBe(3)
  })

  it('stops at the ready message of the daemon, without another look at /health', async () => {
    let calls = 0
    const refused = (async () => {
      calls += 1
      throw new Error('refused')
    }) as typeof fetch
    let ready = (): void => undefined
    const waiting = waitForHealth('http://x/health', {
      timeoutMs: 60_000,
      exited: () => false,
      ready: new Promise<void>((resolve) => {
        ready = resolve
      }),
      fetch: refused,
      stepMs: 60_000,
    })
    await new Promise((resolve) => setTimeout(resolve, 10))
    ready()
    await waiting
    expect(calls).toBe(1)
  })

  it('stops waiting when the daemon exited, or after the time', async () => {
    const refused = (async () => {
      throw new Error('refused')
    }) as typeof fetch
    await expect(
      waitForHealth('http://x/health', { timeoutMs: 2000, exited: () => true, fetch: refused }),
    ).rejects.toThrow(/stopped/)
    await expect(
      waitForHealth('http://x/health', {
        timeoutMs: 20,
        exited: () => false,
        fetch: refused,
        stepMs: 5,
      }),
    ).rejects.toThrow(/did not answer/)
  })

  const PATH_OUT = 'Welcome!\n__NATAMUX_PATH__/opt/homebrew/bin:/usr/bin__NATAMUX_PATH__'

  function runner(error: Error | null, stdout: string, stderr = '') {
    const calls: string[][] = []
    const run = ((_file: string, args: string[], _options: unknown, done: Done) => {
      calls.push(args)
      done(error, stdout, stderr)
    }) as unknown as typeof execFile
    return { run, calls }
  }

  function failure(fields: { killed?: boolean; code?: number | string; signal?: string }): Error {
    return Object.assign(new Error('Command failed'), fields)
  }

  it('reads the PATH of the login shell between its marks', async () => {
    const { run, calls } = runner(null, PATH_OUT)
    expect(await loginShellPath('/bin/zsh', run)).toEqual({ path: '/opt/homebrew/bin:/usr/bin' })
    expect(calls[0]?.[0]).toBe('-ilc')
  })

  it('keeps the PATH when a startup file fails and the shell exits with an error', async () => {
    const { run } = runner(failure({ code: 1 }), PATH_OUT, 'zshrc:112: command not found: mise')
    expect(await loginShellPath('/bin/zsh', run)).toEqual({ path: '/opt/homebrew/bin:/usr/bin' })
  })

  it('keeps the PATH when the shell is killed after printing it', async () => {
    const timedOut = runner(failure({ killed: true, signal: 'SIGTERM' }), PATH_OUT)
    expect(await loginShellPath('/bin/zsh', timedOut.run)).toEqual({
      path: '/opt/homebrew/bin:/usr/bin',
    })
  })

  it('takes only the PATH between the marks, trimmed', async () => {
    const noisy = runner(
      null,
      '\u001b[32mhello\u001b[0m\n__NATAMUX_PATH__ /opt/homebrew/bin:/usr/bin \n__NATAMUX_PATH__\u001b[0m bye',
    )
    expect(await loginShellPath('/bin/zsh', noisy.run)).toEqual({
      path: '/opt/homebrew/bin:/usr/bin',
    })
  })

  it('gives no PATH when a mark is missing or nothing is between the marks', async () => {
    const cut = runner(failure({ killed: true }), 'Welcome!\n__NATAMUX_PATH__/opt/homebrew/bin')
    expect(await loginShellPath('/bin/zsh', cut.run)).toEqual({
      problem: 'it took longer than 10 s',
    })
    for (const stdout of [
      '__NATAMUX_PATH____NATAMUX_PATH__',
      '__NATAMUX_PATH__  __NATAMUX_PATH__',
    ]) {
      const empty = runner(null, stdout)
      expect(await loginShellPath('/bin/zsh', empty.run)).toEqual({
        problem: 'it printed no PATH',
      })
    }
  })

  it('tells why it could not read the PATH', async () => {
    const timedOut = runner(failure({ killed: true }), '')
    expect(await loginShellPath('/bin/zsh', timedOut.run)).toEqual({
      problem: 'it took longer than 10 s',
    })

    const exited = runner(failure({ code: 2 }), 'Welcome!', '\nzshrc:3: parse error\nmore\n')
    expect(await loginShellPath('/bin/zsh', exited.run)).toEqual({
      problem: 'it exited with code 2: zshrc:3: parse error',
    })

    const missing = runner(failure({ code: 'ENOENT' }), '')
    expect(await loginShellPath('/bin/zsh', missing.run)).toEqual({
      problem: 'Command failed',
    })

    const silent = runner(null, 'Welcome!')
    expect(await loginShellPath('/bin/zsh', silent.run)).toEqual({
      problem: 'it printed no PATH',
    })
  })

  it('does not run anything when there is no login shell', async () => {
    const { run, calls } = runner(null, PATH_OUT)
    expect(await loginShellPath(undefined, run)).toEqual({ problem: '$SHELL is not set' })
    expect(await loginShellPath('', run)).toEqual({ problem: '$SHELL is not set' })
    expect(await loginShellPath('   ', run)).toEqual({ problem: '$SHELL is not set' })
    expect(calls).toHaveLength(0)
  })

  it('logs one line with the reason and the PATH used instead, only when the read failed', () => {
    expect(loginPathLogLine({ path: '/opt/homebrew/bin:/usr/bin' }, '/usr/bin')).toBeUndefined()
    const line = loginPathLogLine({ problem: 'it took longer than 10 s' }, '/usr/bin:/bin')
    expect(line).toBe(
      'Could not read the login shell PATH (it took longer than 10 s); using /usr/bin:/bin\n',
    )
    expect(line?.trimEnd().split('\n')).toHaveLength(1)
    expect(loginPathLogLine({ problem: '$SHELL is not set' }, undefined)).toBe(
      'Could not read the login shell PATH ($SHELL is not set); using (none)\n',
    )
  })
})

// Windows has no execute bit; the shim there is a .cmd file instead.
function expectRunnable(file: string): void {
  if (process.platform !== 'win32') {
    expect(statSync(file).mode & 0o111).not.toBe(0)
  }
}

describe('the natamux command', () => {
  it('runs the CLI with the app runtime as Node.js', () => {
    const file = writeCliShim(dir, "/Apps/Nata'mux", '/app/cli/main.js', 'darwin')
    expect(file).toBe(join(dir, 'natamux'))
    const text = readFileSync(file, 'utf8')
    expect(text).toContain(
      "ELECTRON_RUN_AS_NODE=1 exec '/Apps/Nata'\\''mux' '/app/cli/main.js' \"$@\"",
    )
    expectRunnable(file)
  })

  it('also writes the short name nmx', () => {
    writeCliShim(dir, '/Apps/Natamux', '/app/cli/main.js', 'darwin')
    const short = join(dir, 'nmx')
    expect(readFileSync(short, 'utf8')).toBe(readFileSync(join(dir, 'natamux'), 'utf8'))
    expectRunnable(short)
  })

  it('is a .cmd file on Windows', () => {
    const file = writeCliShim(dir, 'C:\\Natamux\\Natamux.exe', 'C:\\cli\\main.js', 'win32')
    expect(file.endsWith('natamux.cmd')).toBe(true)
    expect(readFileSync(file, 'utf8')).toContain('"C:\\Natamux\\Natamux.exe" "C:\\cli\\main.js" %*')
    expect(readFileSync(file, 'utf8')).toContain('setlocal\r\nset ELECTRON_RUN_AS_NODE=1\r\n')
    expect(readFileSync(join(dir, 'nmx.cmd'), 'utf8')).toBe(readFileSync(file, 'utf8'))
  })
})

describe('the desktop smoke', () => {
  it('finds the answer of its command, not the echo of the command itself', () => {
    for (const platform of ['win32', 'darwin'] as const) {
      expect(hasSmokeOutput(`> ${smokeCommandLine(platform)}\r\n`)).toBe(false)
    }
    expect(hasSmokeOutput('echo "$((6*7))xyz"\r\n42xyz\r\n')).toBe(true)
    // The Windows console moves the cursor between parts of one line.
    expect(hasSmokeOutput('\x1b[?25l42\x1b[5;3Hxyz\x1b]0;title\x07')).toBe(true)
  })
})

describe('navigation', () => {
  it('keeps the window on the dashboard and sends the rest to the browser', () => {
    const origin = 'http://127.0.0.1:4317'
    expect(isDashboardUrl('http://127.0.0.1:4317/w/T-0001/changes', origin)).toBe(true)
    expect(isDashboardUrl('http://127.0.0.1:9999/', origin)).toBe(false)
    expect(isDashboardUrl('not a url', origin)).toBe(false)
    expect(opensInBrowser('https://github.com/o/r/pull/5')).toBe(true)
    expect(opensInBrowser('mailto:a@b.c')).toBe(true)
    expect(opensInBrowser('file:///etc/passwd')).toBe(false)
    expect(opensInBrowser('vscode://open')).toBe(false)
  })

  it('accepts the loading page, and only that exact page', () => {
    const origin = 'http://127.0.0.1:4317'
    expect(isDashboardUrl(loadingPageUrl(true), origin)).toBe(true)
    expect(isDashboardUrl(loadingPageUrl(false), origin)).toBe(true)
    expect(isLoadingPage(loadingPageUrl(true))).toBe(true)
    expect(isLoadingPage('http://127.0.0.1:4317/')).toBe(false)
    expect(isDashboardUrl('data:text/html;charset=utf-8,%3Cscript%3E', origin)).toBe(false)
    expect(isDashboardUrl(`${loadingPageUrl(true)}x`, origin)).toBe(false)
    expect(isDashboardUrl('http://127.0.0.1:4317/?task=T-0001#changes', origin)).toBe(true)
    expect(isDashboardUrl('http://127.0.0.1.example.com:4317/', origin)).toBe(false)
  })

  it('paints the loading page in the theme color', () => {
    expect(decodeURIComponent(loadingPageUrl(true))).toContain('background:#18181b')
    expect(decodeURIComponent(loadingPageUrl(false))).toContain('background:#ffffff')
  })
})

describe('the window', () => {
  it('lets the click that brings the app to the front reach the page', () => {
    // Without it, on macOS a click on a terminal from another app only activates the window.
    expect(windowOptions('/app/preload.cjs', false).acceptFirstMouse).toBe(true)
  })

  it('keeps the page isolated from Node', () => {
    expect(windowOptions('/app/preload.cjs', true)).toMatchObject({
      backgroundColor: '#18181b',
      webPreferences: {
        preload: '/app/preload.cjs',
        contextIsolation: true,
        sandbox: true,
        nodeIntegration: false,
      },
    })
  })
})

describe('startup marks', () => {
  it('shows each mark as the time since the first one, in one line', () => {
    const times = [100, 100.4, 160, 400.6]
    const marks = startupMarks(() => times.shift() ?? 0)
    for (const name of ['module start', 'whenReady', 'daemon forked', 'did-finish-load']) {
      marks.mark(name)
    }
    expect(marks.line()).toBe(
      'startup marks: module start 0 ms, whenReady 0 ms, daemon forked 60 ms, did-finish-load 301 ms\n',
    )
  })
})

describe('the shim with spaces in the paths', () => {
  it('quotes the runtime and the CLI on macOS and Linux', () => {
    const file = writeCliShim(dir, '/Apps/My App/Natamux', '/app/my cli/main.js', 'linux')
    expect(readFileSync(file, 'utf8')).toContain(
      "exec '/Apps/My App/Natamux' '/app/my cli/main.js' \"$@\"",
    )
  })

  it('quotes the runtime and the CLI in the .cmd file, for every command', () => {
    const runtime = 'C:\\Program Files\\Natamux\\Natamux.exe'
    const cli = 'C:\\Program Files\\Natamux\\cli\\main.js'
    const file = writeCliShim(dir, runtime, cli, 'win32')
    for (const name of [file, join(dir, 'nmx.cmd')]) {
      expect(readFileSync(name, 'utf8')).toContain(`"${runtime}" "${cli}" %*`)
    }
  })

  it('does not leave ELECTRON_RUN_AS_NODE in the shell that ran the .cmd file', () => {
    const text = readFileSync(writeCliShim(dir, 'C:\\a.exe', 'C:\\m.js', 'win32'), 'utf8')
    expect(text.indexOf('setlocal')).toBeLessThan(text.indexOf('set ELECTRON_RUN_AS_NODE=1'))
  })
})

describe('the dashboard address', () => {
  const origin = 'http://127.0.0.1:4317'

  it('refuses other sites, ports, schemes and look-alike addresses', () => {
    for (const url of [
      'https://example.com/',
      'http://127.0.0.1:4318/',
      'https://127.0.0.1:4317/',
      'http://127.0.0.1:4317@example.com/',
      'http://localhost:4317/',
      'file:///Users/me/index.html',
      'data:text/html,hello',
      'about:blank',
      '',
    ]) {
      expect(isDashboardUrl(url, origin)).toBe(false)
    }
  })

  it('takes a query and a hash on the dashboard, and none on the loading page', () => {
    expect(isDashboardUrl(`${origin}/?a=1#b`, origin)).toBe(true)
    expect(isDashboardUrl(`${loadingPageUrl(true)}#x`, origin)).toBe(false)
    expect(isDashboardUrl(`${loadingPageUrl(false)}?x=1`, origin)).toBe(false)
  })
})

// main.ts needs Electron, so these read its source for the order of the calls. A Windows checkout
// may have CRLF line ends.
const mainSource = () =>
  readFileSync(new URL('./main.ts', import.meta.url), 'utf8').replaceAll('\r\n', '\n')

describe('the start of the desktop app', () => {
  const main = mainSource()
  const at = (text: string) => {
    const index = main.indexOf(text)
    expect(index, text).toBeGreaterThan(-1)
    return index
  }

  it('marks the start in the order it happens', () => {
    const order = [
      "marks.mark('module start')",
      "marks.mark('whenReady')",
      "marks.mark('daemon forked')",
      "marks.mark('daemon ready')",
      "marks.mark('window created')",
      "marks.mark('ready-to-show')",
      "marks.mark('did-finish-load')",
    ].map(at)
    // Only the pairs that run in a row: the daemon, then the window.
    expect(order[0]).toBeLessThan(order[1] ?? 0)
    expect(order[2]).toBeLessThan(order[3] ?? 0)
    expect(order[4]).toBeLessThan(order[5] ?? 0)
    expect(order[5]).toBeLessThan(order[6] ?? 0)
  })

  it('opens the window in the same tick as the fork, on the loading page', () => {
    const forked = at("marks.mark('daemon forked')")
    expect(main.slice(forked, forked + 80)).toContain('onForked()')
    expect(main).toContain('settingsFile)), createWindow)')
    expect(main).toContain('created.loadURL(loadingPageUrl(dark))')
  })

  it('loads the dashboard on ready, and logs the marks without ready-to-show', () => {
    expect(main).toContain('daemonReady = true\n    void window?.loadURL(')
    // The line is written at did-finish-load of the dashboard, not at ready-to-show.
    const load = at("marks.mark('did-finish-load')")
    expect(main.slice(load, load + 60)).toContain('log(marks.line())')
  })

  it('shows an error and quits when the daemon fails to start', () => {
    const failed = at('quitConfirmed = true\n      await dialog.showMessageBox')
    expect(main.slice(failed, failed + 500)).toContain('app.quit()')
  })
})

describe('the preload', () => {
  const preload = readFileSync(new URL('./preload.cts', import.meta.url), 'utf8')

  it('asks the main process for the token and the font with sendSync, not window arguments', () => {
    expect(preload).toContain("sendSync('natamux:daemon-token')")
    expect(preload).toContain("sendSync('natamux:ghostty-font')")
    expect(preload).not.toContain('additionalArguments')
    expect(preload).not.toContain('process.argv')
  })

  it('does not print the token', () => {
    expect(preload).not.toMatch(/console\./)
  })

  it('answers the token only to the dashboard, and the empty text otherwise', () => {
    const main = mainSource()
    const start = main.indexOf("ipcMain.on('natamux:daemon-token'")
    expect(start).toBeGreaterThan(-1)
    const handler = main.slice(start, main.indexOf('})', start))
    expect(handler).toContain('isDashboardUrl(from, new URL(dashboardUrl).origin) ? daemonToken')
    expect(handler).toContain(": ''")
    expect(handler).toContain("event.senderFrame?.url ?? ''")
  })

  it('gives the page only a folder picker, which the main process opens for the dashboard only', () => {
    expect(preload).toContain("contextBridge.exposeInMainWorld('natamux'")
    expect(preload).toContain("invoke('natamux:pick-folder')")
    const main = mainSource()
    const start = main.indexOf("ipcMain.handle('natamux:pick-folder'")
    expect(start).toBeGreaterThan(-1)
    const handler = main.slice(start, main.indexOf('\n})', start))
    expect(handler).toContain("event.senderFrame?.url ?? ''")
    expect(handler).toContain('isDashboardUrl(')
    expect(handler).toContain('pickRepository(')
  })
})

describe('startup marks without some marks', () => {
  it('gives a line with only the marks that were made, as when ready-to-show never comes', () => {
    const times = [10, 20, 35]
    const marks = startupMarks(() => times.shift() ?? 0)
    for (const name of ['module start', 'daemon forked', 'did-finish-load']) {
      marks.mark(name)
    }
    expect(marks.line()).toBe(
      'startup marks: module start 0 ms, daemon forked 10 ms, did-finish-load 25 ms\n',
    )
    expect(marks.line()).not.toContain('ready-to-show')
  })

  it('gives a line when the daemon failed before any other mark', () => {
    const marks = startupMarks(() => 5)
    marks.mark('module start')
    expect(marks.line()).toBe('startup marks: module start 0 ms\n')
    expect(startupMarks().line()).toBe('startup marks: \n')
  })
})
