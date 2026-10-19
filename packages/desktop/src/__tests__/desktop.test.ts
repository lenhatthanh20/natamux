import { readFileSync } from 'node:fs'
import { describe, expect, it } from 'vitest'
import { hasSmokeOutput, smokeCommandLine } from '../smoke.js'

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

// These files need Electron, so these read their source for the order of the calls. A Windows
// checkout may have CRLF line ends.
const sourceOf = (path: string) =>
  readFileSync(new URL(path, import.meta.url), 'utf8').replaceAll('\r\n', '\n')
const mainSource = () => sourceOf('../main.ts')

describe('the start of the desktop app', () => {
  const main = mainSource()
  const daemonSource = sourceOf('../daemon/process.ts')
  const windowSource = sourceOf('../app/window.ts')
  const at = (source: string, text: string) => {
    const index = source.indexOf(text)
    expect(index, text).toBeGreaterThan(-1)
    return index
  }

  it('marks the start in the order it happens', () => {
    // Only the pairs that run in a row: the daemon, then the window.
    expect(at(main, "marks.mark('module start')")).toBeLessThan(at(main, "marks.mark('whenReady')"))
    expect(at(daemonSource, "marks.mark('daemon forked')")).toBeLessThan(
      at(daemonSource, "marks.mark('daemon ready')"),
    )
    const created = at(windowSource, "marks.mark('window created')")
    const shown = at(windowSource, "marks.mark('ready-to-show')")
    expect(created).toBeLessThan(shown)
    expect(shown).toBeLessThan(at(windowSource, "marks.mark('did-finish-load')"))
  })

  it('opens the window in the same tick as the fork, on the loading page', () => {
    const forked = at(daemonSource, "marks.mark('daemon forked')")
    expect(daemonSource.slice(forked, forked + 80)).toContain('onForked()')
    expect(main).toContain('settingsFile)), () => createWindow(ctx))')
    expect(windowSource).toContain('created.loadURL(loadingPageUrl(dark))')
  })

  it('loads the dashboard on ready, and logs the marks without ready-to-show', () => {
    expect(main).toContain('state.daemonReady = true\n    void state.window?.loadURL(')
    // The line is written at did-finish-load of the dashboard, not at ready-to-show.
    const load = at(windowSource, "marks.mark('did-finish-load')")
    expect(windowSource.slice(load, load + 60)).toContain('log(ctx, marks.line())')
  })

  it('shows an error and quits when the daemon fails to start', () => {
    const failed = at(main, 'state.quitConfirmed = true\n      await dialog.showMessageBox')
    expect(main.slice(failed, failed + 500)).toContain('app.quit()')
  })
})

describe('the preload', () => {
  const preload = readFileSync(new URL('../preload.cts', import.meta.url), 'utf8')

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
    expect(handler).toContain('isDashboardUrl(from, new URL(state.dashboardUrl).origin)')
    expect(handler).toContain('? state.daemonToken')
    expect(handler).toContain(": ''")
    expect(handler).toContain("event.senderFrame?.url ?? ''")
  })

  it('gives the page a folder picker and a status report, for the dashboard only', () => {
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

  it('takes the status report only from the dashboard, for the badge and the awake hold', () => {
    expect(preload).toContain("ipcRenderer.send('natamux:status'")
    const main = mainSource()
    const start = main.indexOf("ipcMain.on('natamux:status'")
    expect(start).toBeGreaterThan(-1)
    const handler = main.slice(start, main.indexOf('\n})', start))
    expect(handler).toContain("event.senderFrame?.url ?? ''")
    expect(handler).toContain('isDashboardUrl(')
    expect(handler).toContain('readAgentStatus(')
    expect(handler).toContain('setBadgeCount(')
    expect(handler).toContain('awake.update(')
  })
})

describe('the File menu', () => {
  it('has a File menu item for the awake hold', () => {
    const menu = sourceOf('../app/menu.ts')
    const start = menu.indexOf("label: 'Keep Computer Awake While Agents Run'")
    expect(start).toBeGreaterThan(-1)
    expect(menu.slice(start, start + 300)).toContain("type: 'checkbox'")
  })
})
