import { mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { menuIconsOn } from '../menu-icons.js'
import { isDashboardUrl, isLoadingPage, loadingPageUrl, opensInBrowser } from '../navigation.js'
import {
  autoUpdateOn,
  isGitRepository,
  keepAwakeOn,
  loadSettings,
  readDesktopSettingChange,
  saveSettings,
  startingRepoDir,
} from '../settings.js'
import { windowOptions } from '../window-options.js'

let dir: string

beforeEach(() => {
  dir = mkdtempSync(join(tmpdir(), 'natamux-desktop-'))
})

afterEach(() => {
  rmSync(dir, { recursive: true, force: true })
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

  it('keeps the computer awake unless the user turned it off', () => {
    const file = join(dir, 'settings.json')
    expect(keepAwakeOn(loadSettings(file))).toBe(true)
    saveSettings(file, { keepAwake: false })
    expect(loadSettings(file)).toEqual({ keepAwake: false })
    expect(keepAwakeOn(loadSettings(file))).toBe(false)
    writeFileSync(file, '{"keepAwake":"no"}')
    expect(loadSettings(file)).toEqual({})
    expect(keepAwakeOn(loadSettings(file))).toBe(true)
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

describe('a change from the settings dialog', () => {
  it('takes known booleans only', () => {
    expect(readDesktopSettingChange({ keepAwake: true })).toEqual({ keepAwake: true })
    expect(readDesktopSettingChange({ autoUpdate: false })).toEqual({ autoUpdate: false })
    expect(readDesktopSettingChange({ keepAwake: false, autoUpdate: true })).toEqual({
      keepAwake: false,
      autoUpdate: true,
    })
    expect(readDesktopSettingChange({ keepAwake: true, repoDir: '/x' })).toEqual({
      keepAwake: true,
    })
  })

  it('drops a value that changes nothing', () => {
    for (const value of [null, undefined, 'x', 1, {}, { keepAwake: 'yes' }, { repoDir: '/x' }]) {
      expect(readDesktopSettingChange(value), JSON.stringify(value)).toBeUndefined()
    }
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

  it('shows the dashboard spinner in the center, in the muted ink of the theme', () => {
    const dark = decodeURIComponent(loadingPageUrl(true))
    expect(dark).toContain('<svg')
    expect(dark).toContain('place-items:center')
    expect(dark).toContain('steps(8)')
    expect(dark).toContain('color:#8d919a')
    expect(decodeURIComponent(loadingPageUrl(false))).toContain('color:#5f6570')
  })

  it('shows still dots instead of turning dashes with reduced motion', () => {
    const page = decodeURIComponent(loadingPageUrl(true))
    const reduced = page.slice(page.indexOf('@media (prefers-reduced-motion:reduce)'))
    expect(reduced).toContain('animation:none')
    expect(reduced).toContain('.boot-dots{display:inline}')
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

describe('menu icons', () => {
  it('shows only on macOS 26 and newer, where the system items have icons', () => {
    expect(menuIconsOn('darwin', '26.0.1')).toBe(true)
    expect(menuIconsOn('darwin', '27.1')).toBe(true)
    expect(menuIconsOn('darwin', '15.7.2')).toBe(false)
    expect(menuIconsOn('win32', '10.0.26100')).toBe(false)
    expect(menuIconsOn('linux', '6.8.0')).toBe(false)
  })
})
