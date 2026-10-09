import { describe, expect, it } from 'vitest'
import { withDetailOpen } from './view-store'

describe('withDetailOpen', () => {
  it('sets and changes the flag of one task', () => {
    const opened = withDetailOpen({}, 'T-0001', true)
    expect(opened).toEqual({ 'T-0001': true })
    expect(withDetailOpen(opened, 'T-0001', false)).toEqual({ 'T-0001': false })
  })

  it('keeps the other tasks as they were', () => {
    expect(withDetailOpen({ 'T-0001': true }, 'T-0002', false)).toEqual({
      'T-0001': true,
      'T-0002': false,
    })
  })

  it('forgets the oldest answers when there are too many', () => {
    let flags: Record<string, boolean> = {}
    for (let index = 0; index < 205; index += 1) {
      flags = withDetailOpen(flags, `T-${index}`, true)
    }
    expect(Object.keys(flags)).toHaveLength(200)
    expect(flags['T-0']).toBeUndefined()
    expect(flags['T-204']).toBe(true)
  })
})
