import { describe, expect, it } from 'vitest'
import { fuzzyMatch, splitByRanges } from '../fuzzy'

describe('fuzzyMatch', () => {
  it('matches the characters in order and says where', () => {
    const match = fuzzyMatch('fix', 'Fix the tests')
    expect(match?.ranges).toEqual([[0, 3]])
  })

  it('finds nothing when a character is missing or out of order', () => {
    expect(fuzzyMatch('xyz', 'Fix the tests')).toBeNull()
    expect(fuzzyMatch('tf', 'fix')).toBeNull()
  })

  it('ignores case', () => {
    expect(fuzzyMatch('FIX', 'fix it')).not.toBeNull()
  })

  it('ranks the start of words and neighbouring characters higher', () => {
    const start = fuzzyMatch('ft', 'Fix tests')
    const inside = fuzzyMatch('ft', 'Draft')
    expect(start).not.toBeNull()
    expect(inside).not.toBeNull()
    expect((start?.score ?? 0) > (inside?.score ?? 0)).toBe(true)
    const together = fuzzyMatch('test', 'xtestx')
    const apart = fuzzyMatch('test', 'xtxexsxtx')
    expect((together?.score ?? 0) > (apart?.score ?? 0)).toBe(true)
  })

  it('treats the capitals of a name as word starts', () => {
    expect(fuzzyMatch('tb', 'toggleBoard')?.ranges).toEqual([
      [0, 1],
      [6, 7],
    ])
  })

  it('matches several words in any order', () => {
    const match = fuzzyMatch('tests fix', 'Fix the tests')
    expect(match?.ranges).toEqual([
      [0, 3],
      [8, 13],
    ])
  })

  it('does not find a long query by letters that are far apart', () => {
    expect(fuzzyMatch('stage', 'Show the tab, toggle sidebar, change editor')).toBeNull()
    expect(fuzzyMatch('stage', 'Skip the stage')).not.toBeNull()
    expect(fuzzyMatch('ft', 'Draft')).not.toBeNull()
  })

  it('matches everything for an empty query', () => {
    expect(fuzzyMatch('  ', 'anything')).toEqual({ score: 0, ranges: [] })
  })
})

describe('splitByRanges', () => {
  it('cuts a text into plain and matched pieces', () => {
    expect(
      splitByRanges('Fix the tests', [
        [0, 3],
        [8, 13],
      ]),
    ).toEqual([
      { text: 'Fix', match: true },
      { text: ' the ', match: false },
      { text: 'tests', match: true },
    ])
    expect(splitByRanges('abc', [])).toEqual([{ text: 'abc', match: false }])
  })
})
