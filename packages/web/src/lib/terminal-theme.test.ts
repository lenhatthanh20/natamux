import type { ITheme } from '@xterm/xterm'
import { describe, expect, it } from 'vitest'
import { findDecorations, TERMINAL_LOOK, terminalTheme } from './terminal-theme'

const NAMES = ['black', 'red', 'green', 'yellow', 'blue', 'magenta', 'cyan', 'white'] as const
const COLORS = NAMES.flatMap((name) => [name, `bright${name[0]?.toUpperCase()}${name.slice(1)}`])

// WCAG 2 contrast of two #RRGGBB colors.
function contrast(a: string, b: string): number {
  const luminance = (hex: string) => {
    const [r = 0, g = 0, bl = 0] = [1, 3, 5].map((start) => {
      const value = Number.parseInt(hex.slice(start, start + 2), 16) / 255
      return value <= 0.03928 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4
    })
    return 0.2126 * r + 0.7152 * g + 0.0722 * bl
  }
  const [high, low] = [luminance(a), luminance(b)].sort((x, y) => y - x)
  return ((high ?? 0) + 0.05) / ((low ?? 0) + 0.05)
}

describe('terminalTheme', () => {
  it('has a dark and a light palette with all 16 colors', () => {
    for (const name of ['dark', 'light'] as const) {
      const theme = terminalTheme(name)
      expect(theme.background).toMatch(/^#/)
      expect(theme.brightWhite).toMatch(/^#/)
      expect(theme.black).toMatch(/^#/)
    }
    expect(terminalTheme('dark').background).not.toBe(terminalTheme('light').background)
  })

  it('is the Night palette, with every key xterm reads', () => {
    expect(terminalTheme('dark')).toMatchObject({ background: '#0f1219', foreground: '#d6dcf0' })
    expect(terminalTheme('light')).toMatchObject({ background: '#f7f9fd', foreground: '#1b2238' })
    for (const name of ['dark', 'light'] as const) {
      const theme: ITheme = terminalTheme(name)
      for (const key of [
        'background',
        'foreground',
        'cursor',
        'cursorAccent',
        'selectionBackground',
        'selectionInactiveBackground',
        'scrollbarSliderBackground',
        'scrollbarSliderHoverBackground',
        'scrollbarSliderActiveBackground',
        ...COLORS,
      ] as const) {
        expect(theme[key as keyof ITheme], `${name} ${key}`).toMatch(/^#[0-9a-f]{6}([0-9a-f]{2})?$/)
      }
      expect(theme.cursorAccent).toBe(theme.background)
    }
  })

  it('keeps every named color readable on the background, except black', () => {
    for (const name of ['dark', 'light'] as const) {
      const theme: ITheme = terminalTheme(name)
      const background = theme.background ?? ''
      for (const key of ['foreground', ...COLORS.filter((color) => color !== 'black')]) {
        const color = String(theme[key as keyof ITheme])
        expect(contrast(color, background), `${name} ${key}`).toBeGreaterThanOrEqual(4.5)
      }
    }
  })

  it('gives the scrollbar the same faint colors as the rest of the app', () => {
    expect(terminalTheme('dark').scrollbarSliderBackground).toBe('#ffffff24')
    expect(terminalTheme('light').scrollbarSliderBackground).toBe('#14181f33')
  })

  it('dims the selection of a terminal that does not have the focus', () => {
    for (const name of ['dark', 'light'] as const) {
      const theme = terminalTheme(name)
      expect(theme.selectionInactiveBackground).toMatch(/^#[0-9a-f]{8}$/)
      expect(theme.selectionInactiveBackground).not.toBe(theme.selectionBackground)
    }
  })

  it('returns a copy, so xterm cannot change the palette', () => {
    const first = terminalTheme('dark')
    first.background = '#000000'
    expect(terminalTheme('dark').background).toBe('#0f1219')
  })
})

describe('TERMINAL_LOOK', () => {
  it('has a block cursor, an outline one when unfocused, and bold in its own color', () => {
    expect(TERMINAL_LOOK).toMatchObject({
      cursorStyle: 'block',
      cursorInactiveStyle: 'outline',
      cursorBlink: true,
      drawBoldTextInBrightColors: false,
      fontWeight: 'normal',
      fontWeightBold: 'bold',
      lineHeight: 1,
      minimumContrastRatio: 4.5,
      fastScrollSensitivity: 5,
      overviewRuler: { width: 8 },
    })
  })

  it('keeps out the options that cost speed', () => {
    const look: Record<string, unknown> = TERMINAL_LOOK
    expect(look.smoothScrollDuration ?? 0).toBe(0)
    expect(look.allowTransparency ?? false).toBe(false)
  })
})

describe('findDecorations', () => {
  it('uses #RRGGBB colors, which the search addon requires', () => {
    for (const name of ['dark', 'light'] as const) {
      const colors = findDecorations(name)
      for (const color of Object.values(colors)) {
        expect(color).toMatch(/^#[0-9a-f]{6}$/)
      }
      expect(colors.matchOverviewRuler).toBeDefined()
      expect(colors.activeMatchColorOverviewRuler).toBeDefined()
    }
  })
})
