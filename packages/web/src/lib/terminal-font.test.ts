import { afterEach, describe, expect, it } from 'vitest'
import { ICON_FONT, MONO_FONT, TERMINAL_FONT } from './fonts'
import {
  fontStack,
  GHOSTTY_FONT_KEY,
  parseTerminalFont,
  saveTerminalFont,
  terminalFont,
} from './terminal-font'
import { TERMINAL_FONT_SIZE } from './terminal-theme'

afterEach(() => {
  localStorage.clear()
})

describe('terminal font', () => {
  it('is the built-in font when nothing is set', () => {
    expect(terminalFont()).toMatchObject({
      stack: TERMINAL_FONT,
      size: TERMINAL_FONT_SIZE,
      source: 'default',
    })
  })

  it("uses Ghostty's font, and the user's choice before it, part by part", () => {
    localStorage.setItem(GHOSTTY_FONT_KEY, JSON.stringify({ family: 'JetBrains Mono', size: 14 }))
    expect(terminalFont()).toMatchObject({
      stack: `"JetBrains Mono", ${TERMINAL_FONT}`,
      size: 14,
      source: 'ghostty',
    })

    saveTerminalFont({ size: 16 })
    expect(terminalFont()).toMatchObject({
      stack: `"JetBrains Mono", ${TERMINAL_FONT}`,
      size: 16,
      source: 'custom',
    })

    saveTerminalFont(null)
    expect(terminalFont().source).toBe('ghostty')
  })

  it('is JetBrains Mono, then the icon font; the UI keeps Geist Mono', () => {
    expect(TERMINAL_FONT.startsWith(`"JetBrains Mono Variable", "${ICON_FONT}", `)).toBe(true)
    expect(MONO_FONT.startsWith('"Geist Mono Variable", ')).toBe(true)
  })

  it('keeps the icon font after a font the user picked, for the icons that font lacks', () => {
    expect(fontStack('Menlo')).toBe(`"Menlo", ${TERMINAL_FONT}`)
    expect(fontStack('Menlo')).toContain(ICON_FONT)
  })

  it('keeps quotes out of the family, so the CSS stays one list', () => {
    expect(fontStack('Fira "Code\\')).toBe(`"Fira Code", ${TERMINAL_FONT}`)
    expect(fontStack('  ')).toBe(TERMINAL_FONT)
  })

  it('drops a broken value or a size out of range', () => {
    expect(parseTerminalFont('{ not json')).toBeUndefined()
    expect(parseTerminalFont(JSON.stringify({ size: 200 }))).toBeUndefined()
    expect(parseTerminalFont(JSON.stringify({ family: 'Menlo', size: 2 }))).toEqual({
      family: 'Menlo',
    })
  })
})
