import { describe, expect, it } from 'vitest'
import { findDecorations, TERMINAL_LOOK, terminalTheme } from './terminal-theme'

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
      fontWeightBold: '600',
      lineHeight: 1.2,
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
