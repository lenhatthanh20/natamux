import { describe, expect, it } from 'vitest'
import {
  condense,
  condenseSteps,
  forcedByWidth,
  maxCondenseLevel,
  mergeCondensed,
  type StatusItem,
} from '../status-bar'

const items: StatusItem[] = [
  { id: 'live', priority: 1, hasLabel: true },
  { id: 'repos', priority: 5, hasLabel: true },
  { id: 'import', priority: 6, hasLabel: true },
  { id: 'account', priority: 2, hasLabel: false },
]

describe('condense', () => {
  it('shows everything at level 0', () => {
    const { iconOnly, folded } = condense(items, 0)
    expect(iconOnly.size).toBe(0)
    expect(folded.size).toBe(0)
  })

  it('drops the label of the lowest priority first', () => {
    expect([...condense(items, 1).iconOnly]).toEqual(['import'])
    expect([...condense(items, 2).iconOnly]).toEqual(['import', 'repos'])
  })

  it('drops every label before it folds an item', () => {
    const afterLabels = condense(items, 3)
    expect([...afterLabels.iconOnly]).toEqual(['import', 'repos', 'live'])
    expect(afterLabels.folded.size).toBe(0)
  })

  it('then folds the lowest priority first', () => {
    expect([...condense(items, 4).folded]).toEqual(['import'])
    expect([...condense(items, 5).folded]).toEqual(['import', 'repos'])
  })

  it('has one step per label and one per item', () => {
    expect(maxCondenseLevel(items)).toBe(3 + 4)
    expect(condenseSteps(items)).toHaveLength(7)
  })

  it('keeps the most important item to the very end', () => {
    const last = condenseSteps(items).at(-1)
    expect(last).toEqual({ id: 'live', kind: 'fold' })
  })
})

describe('forcedByWidth', () => {
  it('forces nothing when the width is unknown or large', () => {
    for (const width of [null, 1280, 1920]) {
      const { iconOnly, folded } = forcedByWidth(width)
      expect(iconOnly.size + folded.size).toBe(0)
    }
  })

  it('turns Projects and Import into icons from 1024 to 1279', () => {
    const { iconOnly, folded } = forcedByWidth(1100)
    expect([...iconOnly].sort()).toEqual(['import', 'repos'])
    expect(folded.size).toBe(0)
  })

  it('keeps only the main items under 1024', () => {
    const { iconOnly, folded } = forcedByWidth(800)
    expect(iconOnly.has('live')).toBe(true)
    expect([...folded].sort()).toEqual(['import', 'notifications', 'repos'])
  })

  it('merges with the measured result', () => {
    const merged = mergeCondensed(forcedByWidth(1100), condense(items, 4))
    expect([...merged.iconOnly].sort()).toEqual(['import', 'live', 'repos'])
    expect([...merged.folded]).toEqual(['import'])
  })
})
