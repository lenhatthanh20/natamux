import { cleanup, render } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { ShapeSvg } from '../ShapeGlyph'
import { StatusGlyph } from '../StatusGlyph'

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
})

describe('StatusGlyph', () => {
  it('draws the same shape as the sidebar, at 10 px in a 16 px box', () => {
    const { container } = render(<StatusGlyph state="needs-input" />)
    const glyph = container.firstElementChild as HTMLElement
    expect(glyph.className).toContain('size-4')
    const shape = render(<ShapeSvg kind="needs-input" art={10} pad={2} />).container
    expect(glyph.querySelector('svg')?.outerHTML).toBe(shape.querySelector('svg')?.outerHTML)
  })

  it('blinks while running, with no lucide spinner', () => {
    const { container } = render(<StatusGlyph state="running" />)
    expect(container.querySelector('svg.hm-beat')).toBeTruthy()
    expect(container.querySelector('.lucide')).toBeNull()
    expect(container.querySelector('.animate-spin')).toBeNull()
  })

  // Reduced motion is CSS only: no listener per glyph.
  it('does not read the reduced-motion media query', () => {
    const matchMedia = vi.fn()
    vi.stubGlobal('matchMedia', matchMedia)
    render(<StatusGlyph state="running" />)
    expect(matchMedia).not.toHaveBeenCalled()
  })

  it('shows a quiet agent in the needs-you color, still', () => {
    const { container } = render(<StatusGlyph state="running" quiet tip="Implement running" />)
    const glyph = container.firstElementChild as HTMLElement
    expect(glyph.dataset.state).toBe('needs-input')
    expect(glyph.dataset.tip).toBe('Running, but quiet · Implement running')
    expect(glyph.querySelector('.hm-beat')).toBeNull()
  })
})
