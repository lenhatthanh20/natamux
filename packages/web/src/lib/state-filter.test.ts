import { describe, expect, it } from 'vitest'
import { chipsThatFit } from './state-filter'

describe('chipsThatFit', () => {
  it('shows every chip when they all fit, without room for "+N"', () => {
    expect(chipsThatFit([80, 80, 80], 244, 40)).toBe(3)
  })

  it('folds the chips that do not fit and keeps room for "+N"', () => {
    // 80 + 2 + 80 + 2 + 40 (the button) = 204 fits in 210; a third chip would not.
    expect(chipsThatFit([80, 80, 80], 210, 40)).toBe(2)
  })

  it('can fold every chip', () => {
    expect(chipsThatFit([80, 80], 50, 40)).toBe(0)
  })

  it('has nothing to fit without chips', () => {
    expect(chipsThatFit([], 100, 40)).toBe(0)
  })
})
