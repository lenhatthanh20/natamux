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

type Done = (error: Error | null, stdout: string) => void

let dir: string

beforeEach(() => {
  dir = mkdtempSync(join(tmpdir(), 'helmsman-desktop-'))
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

  it('gives the daemon its place, and the shells the helmsman command first on the PATH', () => {
    const env = daemonEnv({ PATH: '/usr/bin', HOME: '/home/me' }, 'secret', place)
    expect(env).toMatchObject({
      HOME: '/home/me',
      HELMSMAN_HOST: '127.0.0.1',
      HELMSMAN_PORT: '4318',
      HELMSMAN_TOKEN: 'secret',
      HELMSMAN_DATA_DIR: '/data',
      HELMSMAN_REPO_DIR: '/repo',
      HELMSMAN_WEB_DIR: '/web',
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

  it('reads the PATH of the login shell between its marks', async () => {
    const run = ((_file: string, args: string[], _options: unknown, done: Done) => {
      expect(args[0]).toBe('-ilc')
      done(null, 'Welcome!\n__HELMSMAN_PATH__/opt/homebrew/bin:/usr/bin__HELMSMAN_PATH__')
    }) as unknown as typeof execFile
    expect(await loginShellPath('/bin/zsh', run)).toBe('/opt/homebrew/bin:/usr/bin')
    expect(await loginShellPath(undefined, run)).toBeUndefined()

    const failing = ((_file: string, _args: string[], _options: unknown, done: Done) =>
      done(new Error('timeout'), '')) as unknown as typeof execFile
    expect(await loginShellPath('/bin/zsh', failing)).toBeUndefined()
  })
})

describe('the helmsman command', () => {
  it('runs the CLI with the app runtime as Node.js', () => {
    const file = writeCliShim(dir, "/Apps/Helms'man", '/app/cli/main.js', 'darwin')
    const text = readFileSync(file, 'utf8')
    expect(text).toContain(
      "ELECTRON_RUN_AS_NODE=1 exec '/Apps/Helms'\\''man' '/app/cli/main.js' \"$@\"",
    )
    expect(statSync(file).mode & 0o111).not.toBe(0)
  })

  it('is a .cmd file on Windows', () => {
    const file = writeCliShim(dir, 'C:\\Helmsman\\Helmsman.exe', 'C:\\cli\\main.js', 'win32')
    expect(file.endsWith('helmsman.cmd')).toBe(true)
    expect(readFileSync(file, 'utf8')).toContain(
      '"C:\\Helmsman\\Helmsman.exe" "C:\\cli\\main.js" %*',
    )
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
