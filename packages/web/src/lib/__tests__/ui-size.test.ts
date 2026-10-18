import { describe, expect, it } from 'vitest'
import { parseUiSize, uiSizeZoom } from '../ui-size'

describe('ui size', () => {
  it('reads the stored value and falls back to the default', () => {
    expect(parseUiSize('compact')).toBe('compact')
    expect(parseUiSize('large')).toBe('large')
    expect(parseUiSize('huge')).toBe('default')
    expect(parseUiSize(null)).toBe('default')
  })

  it('maps the sizes to the zoom of the spec', () => {
    expect(uiSizeZoom('compact')).toBe(0.92)
    expect(uiSizeZoom('default')).toBe(1)
    expect(uiSizeZoom('large')).toBe(1.077)
  })
})
