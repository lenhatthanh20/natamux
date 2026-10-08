import { describe, expect, it } from 'vitest'
import { terminalTheme } from './terminal-theme'

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

  it('returns a copy, so xterm cannot change the palette', () => {
    const first = terminalTheme('dark')
    first.background = '#000000'
    expect(terminalTheme('dark').background).toBe('#0b0c0e')
  })
})
