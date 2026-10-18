import { readFileSync } from 'node:fs'
import { describe, expect, it } from 'vitest'
import {
  checkingView,
  downloadingView,
  POPUP_CSP,
  popupWindowOptions,
  showScript,
  updatePopupUrl,
} from '../update-popup.js'

const html = decodeURIComponent(updatePopupUrl().replace(/^data:text\/html;charset=utf-8,/, ''))

describe('the update popup page', () => {
  it('is a data: URL that loads nothing from outside', () => {
    expect(updatePopupUrl().startsWith('data:text/html;charset=utf-8,')).toBe(true)
    expect(html).toContain(`<meta http-equiv="Content-Security-Policy" content="${POPUP_CSP}">`)
    expect(POPUP_CSP).toBe(
      "default-src 'none'; style-src 'unsafe-inline'; script-src 'unsafe-inline'",
    )
    expect(html).not.toMatch(/https?:\/\//)
    expect(html).not.toMatch(/<script[^>]*\ssrc=/)
  })

  it('uses the system font, not Inter', () => {
    expect(html).toContain('font-family:-apple-system, "Segoe UI", system-ui, sans-serif')
    expect(html).not.toMatch(/\bInter\b/)
  })

  it('defines every color in both schemes', () => {
    const blocks = [...html.matchAll(/:root\{([^}]*)\}/g)].map((match) => match[1] ?? '')
    expect(blocks).toHaveLength(2)
    const names = (block: string) => [...block.matchAll(/(--[\w-]+):/g)].map((match) => match[1])
    const [light = '', dark = ''] = blocks
    expect(names(light).length).toBeGreaterThanOrEqual(6)
    expect(names(dark).sort()).toEqual(names(light).sort())
    for (const [, used] of html.matchAll(/var\((--[\w-]+)\)/g)) {
      expect(names(light)).toContain(used)
    }
  })

  it('follows the system theme with the dashboard colors, and can stand still', () => {
    expect(html).toContain('#2f5fe0')
    expect(html).toContain('@media (prefers-color-scheme: dark)')
    expect(html).toContain('#7aa2ff')
    expect(html).toContain('@media (prefers-reduced-motion: reduce)')
  })

  it('closes on Hide and on Esc', () => {
    expect(html).toContain('>Hide</button>')
    expect(html).toContain("'Escape'")
    expect(html).toContain('window.close()')
  })
})

describe('the popup views', () => {
  it('shows the running version while it checks', () => {
    expect(checkingView('0.1.24')).toEqual({
      kind: 'checking',
      title: 'Checking for updates…',
      detail: 'You have Natamux 0.1.24',
      hint: '',
    })
  })

  it('shows the download in MB with a percent', () => {
    expect(downloadingView('0.1.25', 62_400_000, 118_200_000)).toEqual({
      kind: 'downloading',
      title: 'Downloading Natamux 0.1.25…',
      detail: '62 of 118 MB',
      hint: 'You can keep working. It installs on restart.',
      percent: 52,
    })
    expect(downloadingView('0.1.25', 0, 118_200_000).percent).toBe(0)
    expect(downloadingView('0.1.25', 118_200_000, 118_200_000).percent).toBe(100)
  })

  it('never shows more done than the total, and rounds both the same way', () => {
    expect(downloadingView('0.1.25', 130_000_000, 118_200_000)).toMatchObject({
      detail: '118 of 118 MB',
      percent: 100,
    })
    expect(downloadingView('0.1.25', 118_600_000, 118_600_000).detail).toBe('119 of 119 MB')
    expect(downloadingView('0.1.25', 0, 118_200_000).detail).toBe('0 of 118 MB')
    expect(downloadingView('0.1.25', 5e12, 6e12).detail).toBe('5000000 of 6000000 MB')
  })

  it('does not throw on negative or NaN input', () => {
    expect(downloadingView('0.1.25', -5, 118_200_000)).toMatchObject({
      detail: '0 of 118 MB',
      percent: 0,
    })
    expect(downloadingView('0.1.25', Number.NaN, 118_200_000).detail).toBe('0 of 118 MB')
    const unknown = downloadingView('0.1.25', Number.NaN, Number.NaN)
    expect(unknown.detail).toBe('0 MB')
    expect(unknown.percent).toBeUndefined()
    expect(downloadingView('0.1.25', 1_000_000, -1).detail).toBe('1 MB')
  })

  it('shows only the MB done when the size is unknown', () => {
    const view = downloadingView('0.1.25', 62_400_000, undefined)
    expect(view.detail).toBe('62 MB')
    expect(view.percent).toBeUndefined()
    expect(downloadingView('0.1.25', 5_000_000, 0).percent).toBeUndefined()
  })
})

describe('showScript', () => {
  it('passes the view as JSON, so text cannot break out of the call', () => {
    const view = { ...checkingView('1.0.0'), detail: '"); window.close(); ("</script>' }
    const script = showScript(view)
    expect(script).toBe(`show(${JSON.stringify(view)})`)
    expect(JSON.parse(script.slice('show('.length, -1))).toEqual(view)
  })
})

describe('popupWindowOptions', () => {
  it('is a small sandboxed window with no preload', () => {
    const options = popupWindowOptions(false, undefined)
    expect(options.frame).toBe(false)
    expect(options.resizable).toBe(false)
    expect(options.skipTaskbar).toBe(true)
    expect(options.show).toBe(false)
    expect(options.backgroundColor).toBe('#ffffff')
    expect(popupWindowOptions(true, undefined).backgroundColor).toBe('#1c1d21')
    expect(options.webPreferences).toEqual({
      sandbox: true,
      contextIsolation: true,
      nodeIntegration: false,
      spellcheck: false,
    })
    expect(options.x).toBeUndefined()
  })

  it('sits in the middle of the main window', () => {
    const options = popupWindowOptions(false, { x: 100, y: 50, width: 1440, height: 900 })
    expect(options.x).toBe(100 + (1440 - (options.width ?? 0)) / 2)
    expect(options.y).toBe(50 + Math.round((900 - (options.height ?? 0)) / 3))
  })
})

describe('the updater', () => {
  const source = readFileSync(new URL('../updater.ts', import.meta.url), 'utf8')
  const indexes = (pattern: RegExp) => [...source.matchAll(pattern)].map((match) => match.index)

  // Timed checks stay quiet.
  it('opens the popup only for a manual check', () => {
    expect(source.match(/this\.openPopup\(\)/g)).toHaveLength(1)
    expect(source).toMatch(/if \(manual\) \{\s*this\.openPopup\(\)/)
  })

  it('closes the popup before every dialog', () => {
    const dialogs = indexes(/dialog\.showMessageBox\(/g)
    const closes = indexes(/await this\.closePopup\(\)/g)
    // restart() runs after a dialog was answered, so no popup is open there.
    const restart = source.indexOf('private async restart(')
    expect(restart).toBeGreaterThan(-1)
    const checked = dialogs.filter((index) => index < restart)
    expect(checked.length).toBeGreaterThanOrEqual(4)
    for (const at of checked) {
      const before = closes.filter((index) => index < at).pop()
      expect(before, `a closePopup before the dialog at ${at}`).toBeDefined()
      expect(source.slice(before, at)).not.toMatch(/\n {2}(private )?(async )?\w+\(.*\): Promise/)
    }
  })
})
