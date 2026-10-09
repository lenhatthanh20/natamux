import { describe, expect, it } from 'vitest'
import { findCountLabel } from './terminal-find'

describe('findCountLabel', () => {
  it('says nothing before a search', () => {
    expect(findCountLabel('', null)).toBe('')
    expect(findCountLabel('', { index: 0, count: 3 })).toBe('')
  })

  it('counts the matches from one', () => {
    expect(findCountLabel('foo', { index: 0, count: 12 })).toBe('1 of 12')
    expect(findCountLabel('foo', { index: 11, count: 12 })).toBe('12 of 12')
    expect(findCountLabel('foo', { index: -1, count: 4 })).toBe('4 found')
  })

  it('says when there is no match', () => {
    expect(findCountLabel('foo', { index: -1, count: 0 })).toBe('No results')
    expect(findCountLabel('foo', null)).toBe('No results')
  })
})
