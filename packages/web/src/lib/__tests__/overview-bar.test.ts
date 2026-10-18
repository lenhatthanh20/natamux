import { describe, expect, it } from 'vitest'
import css from '../../styles.css?raw'

const SOURCES = import.meta.glob<string>(['../../**/*.{ts,tsx}', '!../../**/__tests__/**'], {
  eager: true,
  query: '?raw',
  import: 'default',
})

// The Overview card of the progress had a bar of stages with moving stripes. It cost CPU while a
// stage ran, so it is gone, with its styles.
describe('the removed stage bar', () => {
  it('leaves no stripes in the styles', () => {
    expect(css).not.toContain('stage-stripes')
    expect(css).not.toContain('hm-stripes')
  })

  it('leaves every selector list whole', () => {
    expect(css).not.toMatch(/,\s*,/)
    expect(css).not.toMatch(/,\s*\{/)
  })

  it('is used by no component', () => {
    expect(Object.keys(SOURCES).length).toBeGreaterThan(0)
    for (const [path, text] of Object.entries(SOURCES)) {
      expect(text, path).not.toContain('stage-stripes')
    }
  })
})
