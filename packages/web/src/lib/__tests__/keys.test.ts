import { describe, expect, it } from 'vitest'
import { formatShortcut } from '../keys'

describe('formatShortcut', () => {
  it('uses glyphs in system order on macOS', () => {
    expect(formatShortcut('Mod+K', true)).toBe('⌘K')
    expect(formatShortcut('Mod+Shift+P', true)).toBe('⇧⌘P')
    expect(formatShortcut('Mod+Shift+Enter', true)).toBe('⇧⌘↵')
    expect(formatShortcut('Mod+Alt+B', true)).toBe('⌥⌘B')
  })

  it('uses words joined with + on Windows', () => {
    expect(formatShortcut('Mod+K', false)).toBe('Ctrl+K')
    expect(formatShortcut('Mod+Shift+Enter', false)).toBe('Ctrl+Shift+Enter')
    expect(formatShortcut('Ctrl+Shift+`', false)).toBe('Ctrl+Shift+`')
  })

  it('shows a single key as it is', () => {
    expect(formatShortcut('c', false)).toBe('C')
    expect(formatShortcut('Esc', true)).toBe('Esc')
    expect(formatShortcut('Mod+,', false)).toBe('Ctrl+,')
  })
})
