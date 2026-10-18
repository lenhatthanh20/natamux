import { describe, expect, it } from 'vitest'
import css from '../../styles.css?raw'
import { MOTION_TICK_MS } from '../motion-ticker'

// The motion of the status icons (docs/design-icons.md). The app draws endless animations at
// 8 frames a second, so they are built on its 125 ms ticks and stay on the compositor.
const ENDLESS = /^([^{}@\n][^{}]*)\{[^}]*?animation:\s*([\w-]+)\s+(\d+)ms[^;]*infinite/gm

const endless = [...css.matchAll(ENDLESS)].map((match) => ({
  // The last line is the selector; the lines above it can be a comment.
  selectors: (match[1] ?? '')
    .trim()
    .split('\n')
    .at(-1)
    ?.split(',')
    .map((selector) => selector.trim()),
  name: match[2] ?? '',
  ms: Number(match[3]),
}))

const keyframes = (name: string): string => {
  const start = css.indexOf(`@keyframes ${name} {`)
  if (start < 0) {
    return ''
  }
  let depth = 0
  for (let index = css.indexOf('{', start); index < css.length; index++) {
    if (css[index] === '{') {
      depth++
    } else if (css[index] === '}') {
      depth--
      if (depth === 0) {
        return css.slice(start, index + 1)
      }
    }
  }
  return ''
}

const reduced = css
  .split('@media (prefers-reduced-motion: reduce)')
  .slice(1)
  .map((block) => block.slice(0, block.indexOf('\n}')))
  .join('\n')

const inactive = css
  .split(/\n(?=\S)/)
  .filter((block) => block.startsWith(':root[data-window="inactive"]'))
  .join('\n')

const MOVING = ['.hm-beat', '.hm-spin8', '.hm-pulse', '.stage-link[data-kind="flow"]::before']

describe('icon motion', () => {
  it.each([
    ['.hm-beat', 2000],
    ['.hm-spin8', 1000],
    ['.hm-pulse', 2000],
    ['.stage-link[data-kind="flow"]::before', 2000],
  ])('runs %s for %i ms, a whole number of ticks', (selector, ms) => {
    const rule = endless.find((entry) => entry.selectors?.includes(selector))
    expect(rule?.ms).toBe(ms)
    expect((rule?.ms ?? 1) % MOTION_TICK_MS).toBe(0)
  })

  // background-position, filter or box-shadow would repaint on every frame.
  it('moves only transform and opacity in an endless animation', () => {
    expect(endless.length).toBeGreaterThan(0)
    for (const { name } of endless) {
      const body = keyframes(name)
      expect(body, name).not.toBe('')
      for (const match of body.matchAll(/([a-z-]+)\s*:/g)) {
        expect(['transform', 'opacity'], `${name}: ${match[1]}`).toContain(match[1])
      }
    }
  })

  it('keeps no shimmer, spinning arc or halo', () => {
    expect(css).not.toContain('@keyframes hm-shimmer')
    expect(css).not.toContain('.stage-arc')
    expect(css).not.toContain('.stage-halo')
    expect(css).not.toContain('--animate-spin')
  })

  it.each(MOVING)('stops %s with reduced motion and in a window without focus', (selector) => {
    expect(reduced).toContain(selector)
    expect(inactive).toContain(selector)
  })

  // The still spinner is three dots, not a dash pattern frozen mid-turn.
  it('shows three dots for the still spinner', () => {
    for (const still of [reduced, inactive]) {
      expect(still).toContain('.hm-spin8 .hm-spin-dashes')
      expect(still).toContain('.hm-spin8 .hm-spin-dots')
    }
  })

  // Lines keep the same width on screen at every icon size, as lucide's absoluteStrokeWidth.
  it('sets the stroke of action icons in screen pixels', () => {
    expect(css).toMatch(/\.lucide \*\s*\{\s*vector-effect: non-scaling-stroke;/)
    expect(css).toMatch(/\.lucide\s*\{\s*stroke-width: 1\.25px;/)
    expect(css).toMatch(
      /\.lucide\[width="20"\],\s*\.lucide\[width="32"\]\s*\{\s*stroke-width: 1\.5px;/,
    )
  })
})
