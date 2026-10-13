import { describe, expect, it } from 'vitest'
import css from '../styles.css?raw'

// Every sidebar animation has a calm form for users who ask for less motion.
describe('sidebar motion', () => {
  const classes = [...new Set(css.match(/\.sb-[a-z-]+/g) ?? [])]
  const reduced = css
    .split('@media (prefers-reduced-motion: reduce)')
    .slice(1)
    .map((block) => block.slice(0, block.indexOf('\n}')))
    .join('\n')

  it('has sidebar classes', () => {
    expect(classes.length).toBeGreaterThan(0)
  })

  it.each(classes)('turns %s down when motion is reduced', (name) => {
    expect(reduced).toContain(name)
  })

  it('stops the halo and the sparks of the comet', () => {
    expect(reduced).toContain('.comet[data-state="running"] .comet-halo')
    expect(reduced).toContain('.comet[data-state="running"] .comet-spark')
  })
})
