import { describe, expect, it } from 'vitest'
import { parseTheme, resolveTheme } from './theme'

describe('parseTheme', () => {
  it('keeps light and dark and treats anything else as system', () => {
    expect(parseTheme('light')).toBe('light')
    expect(parseTheme('dark')).toBe('dark')
    expect(parseTheme(null)).toBe('system')
    expect(parseTheme('blue')).toBe('system')
  })
})

describe('resolveTheme', () => {
  it('follows the system only for the system choice', () => {
    expect(resolveTheme('system', true)).toBe('dark')
    expect(resolveTheme('system', false)).toBe('light')
    expect(resolveTheme('light', true)).toBe('light')
    expect(resolveTheme('dark', false)).toBe('dark')
  })
})
