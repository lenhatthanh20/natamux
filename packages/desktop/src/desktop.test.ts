import type { execFile } from 'node:child_process'
import { mkdtempSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs'
import { createServer } from 'node:net'
import { tmpdir } from 'node:os'
import { delimiter, join } from 'node:path'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { writeCliShim } from './cli-shim.js'
import { loadOrCreateDaemonToken } from './daemon-access.js'
import { daemonEnv } from './daemon-env.js'
import { isDashboardUrl, opensInBrowser } from './navigation.js'
import { isGitRepository, loadSettings, saveSettings } from './settings.js'
import { freePort, loginShellPath, waitForHealth } from './startup.js'

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

  it('knows a git repository by its .git', () => {
    expect(isGitRepository(dir)).toBe(false)
    writeFileSync(join(dir, '.git'), 'gitdir: /elsewhere\n')
    expect(isGitRepository(dir)).toBe(true)
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

  it('keeps the app PATH when the login shell gave none, and drops ELECTRON_RUN_AS_NODE', () => {
    const env = daemonEnv({ PATH: '/usr/bin', ELECTRON_RUN_AS_NODE: '1' }, 'secret', {
      ...place,
      loginPath: undefined,
    })
    expect(env.PATH).toBe(['/app/bin', '/usr/bin'].join(delimiter))
    expect('ELECTRON_RUN_AS_NODE' in env).toBe(false)
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

  function failure(fields: { killed?: boolean; code?: number | string }): Error {
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
    expect(calls).toHaveLength(0)
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
    expect(readFileSync(join(dir, 'nmx.cmd'), 'utf8')).toBe(readFileSync(file, 'utf8'))
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
})
