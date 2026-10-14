import { TASK_STATES } from '@natamux/core'
import { cleanup, render } from '@testing-library/react'
import { afterEach, describe, expect, it } from 'vitest'
import { ShapeGlyph, type ShapeKind, ShapeSvg } from './ShapeGlyph'

afterEach(cleanup)

const KINDS: ShapeKind[] = [...TASK_STATES, 'quiet', 'unknown']

const markup = (kind: ShapeKind, art: number) => {
  const { container } = render(<ShapeSvg kind={kind} art={art} pad={1} />)
  const html = container.querySelector('svg')?.innerHTML ?? ''
  cleanup()
  return html
}

describe('ShapeGlyph', () => {
  // Each state has its own silhouette, so a row reads without color too.
  it.each([8, 10])('draws a different shape for every state at %i px', (art) => {
    const shapes = KINDS.map((kind) => markup(kind, art))
    expect(new Set(shapes).size).toBe(KINDS.length)
  })

  it('draws on whole pixels: the box is the art size plus the padding', () => {
    const { container } = render(<ShapeSvg kind="review" art={10} pad={2} />)
    const svg = container.querySelector('svg')
    expect(svg?.getAttribute('width')).toBe('14')
    expect(svg?.getAttribute('viewBox')).toBe('-2 -2 14 14')
    expect(svg?.getAttribute('data-shape')).toBe('review')
  })

  it('makes only a running glyph move', () => {
    for (const kind of KINDS) {
      const { container } = render(<ShapeSvg kind={kind} art={10} pad={2} />)
      const moves = container.querySelector('svg')?.classList.contains('hm-beat')
      expect(moves).toBe(kind === 'running')
      cleanup()
    }
  })

  it('is 10 px by default and 8 px on request', () => {
    const { container, rerender } = render(<ShapeGlyph state="ready" />)
    expect(container.querySelector('svg')?.getAttribute('width')).toBe('14')
    rerender(<ShapeGlyph state="ready" size={8} />)
    expect(container.querySelector('svg')?.getAttribute('width')).toBe('10')
  })

  it('shows a quiet agent as a half disk in the needs-you color', () => {
    const { container } = render(<ShapeGlyph state="running" quiet />)
    const glyph = container.firstElementChild as HTMLElement
    expect(glyph.dataset.state).toBe('needs-input')
    expect(glyph.dataset.tip).toBe('Running, but quiet')
    expect(glyph.querySelector('svg')?.getAttribute('data-shape')).toBe('quiet')
    expect(glyph.querySelector('.hm-beat')).toBeNull()
  })

  it('shows a state that is not known yet as a muted ring with a dot, as wide as the ring', () => {
    const { container } = render(<ShapeGlyph state={null} size={8} />)
    const glyph = container.firstElementChild as HTMLElement
    expect(glyph.dataset.state).toBeUndefined()
    expect(glyph.dataset.tip).toBe('State not known yet')
    expect(glyph.className).toContain('text-muted')
    expect(glyph.querySelector('svg')?.getAttribute('data-shape')).toBe('unknown')
    const [ring, dot] = [...glyph.querySelectorAll('circle')]
    expect(ring?.getAttribute('fill')).toBe('none')
    expect(ring?.getAttribute('r')).toBe('3.25')
    expect(ring?.getAttribute('stroke-width')).toBe('1.5')
    expect(dot?.getAttribute('fill')).toBe('currentColor')
    expect(dot?.getAttribute('r')).toBe('1')
  })

  it.each([...TASK_STATES])('names the %s state for screen readers', (state) => {
    const { container } = render(<ShapeGlyph state={state} tip="Implement running" />)
    const glyph = container.firstElementChild as HTMLElement
    expect(glyph.dataset.state).toBe(state)
    expect(glyph.querySelector('.sr-only')?.textContent).not.toBe('')
    expect(glyph.dataset.tip).toContain('Implement running')
  })
})
