import { afterEach, describe, expect, it, vi } from 'vitest'
import css from '../../styles.css?raw'
import { startWindowFocus } from '../window-focus'

describe('window focus', () => {
  afterEach(() => {
    vi.restoreAllMocks()
    document.documentElement.removeAttribute('data-window')
  })

  it('marks the page inactive on blur and clears it on focus', () => {
    const focused = vi.spyOn(document, 'hasFocus').mockReturnValue(true)
    const stop = startWindowFocus()
    expect(document.documentElement.dataset.window).toBeUndefined()

    focused.mockReturnValue(false)
    window.dispatchEvent(new Event('blur'))
    expect(document.documentElement.dataset.window).toBe('inactive')

    focused.mockReturnValue(true)
    window.dispatchEvent(new Event('focus'))
    expect(document.documentElement.dataset.window).toBeUndefined()
    stop()
  })

  it('starts inactive when the window opens without focus', () => {
    vi.spyOn(document, 'hasFocus').mockReturnValue(false)
    const stop = startWindowFocus()
    expect(document.documentElement.dataset.window).toBe('inactive')
    stop()
  })

  // Every endless animation must stop, or the browser keeps drawing every frame.
  it('stops every endless animation while inactive', () => {
    const endless = [...css.matchAll(/^([^{}@\n][^{}]*)\{[^}]*animation:[^;]*infinite/gm)]
      .map((match) => (match[1] ?? '').trim().split('\n').at(-1) ?? '')
      // The skeleton only shows while something loads.
      .filter((selector) => selector !== '.hm-skeleton')
    const inactive = css
      .split(/\n(?=\S)/)
      .filter((block) => block.startsWith(':root[data-window="inactive"]'))
      .join('\n')
    expect(endless.length).toBeGreaterThan(0)
    for (const selector of endless) {
      expect(inactive).toContain(selector)
    }
  })
})
