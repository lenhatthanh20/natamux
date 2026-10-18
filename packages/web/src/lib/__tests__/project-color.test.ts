import { describe, expect, it } from 'vitest'
import { PROJECT_COLORS, projectColor } from '../project-color'

describe('projectColor', () => {
  it('gives a project the same color every time', () => {
    expect(projectColor('web')).toBe(projectColor('web'))
    expect(projectColor('default')).toBe('var(--project-3)')
    expect(projectColor('web')).toBe('var(--project-0)')
  })

  it('always picks one of the palette colors', () => {
    for (const id of ['default', 'web', 'a', 'x'.repeat(60), 'my-very-long-project-name']) {
      expect(projectColor(id)).toMatch(new RegExp(`^var\\(--project-[0-${PROJECT_COLORS - 1}]\\)$`))
    }
  })

  it('spreads projects over the palette', () => {
    const ids = ['default', 'web', 'old', 'natamux', 'infra', 'docs']
    expect(new Set(ids.map(projectColor)).size).toBeGreaterThanOrEqual(5)
  })
})
