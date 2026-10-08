import { EVENT_TYPES } from '@helmsman/core'
import { describe, expect, it } from 'vitest'
import { describeEvent, formatClock } from './events'

describe('describeEvent', () => {
  it('has a sentence for every kind of event', () => {
    for (const type of EVENT_TYPES) {
      expect(describeEvent(type).length, type).toBeGreaterThan(0)
    }
  })

  it('does not give two kinds of event the same sentence', () => {
    const sentences = EVENT_TYPES.map((type) => describeEvent(type))
    expect(new Set(sentences).size).toBe(sentences.length)
  })
})

describe('formatClock', () => {
  it('shows hours, minutes and seconds', () => {
    expect(formatClock('2026-10-08T09:05:07.000Z')).toMatch(/\d{1,2}\D\d{2}\D\d{2}/)
  })
})
