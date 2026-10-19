import { describe, expect, it } from 'vitest'
import { sliceEnd, splitFrames } from '../screen-state.js'

describe('splitFrames', () => {
  it('splits bytes into frames of at most the given size, and no bytes into no frame', () => {
    expect(splitFrames(Buffer.from('abcdefg'), 3).map(String)).toEqual(['abc', 'def', 'g'])
    expect(splitFrames(Buffer.alloc(0))).toEqual([])
  })

  it.each([
    ['an emoji', '😀'],
    ['a Vietnamese letter', 'ệ'],
    ['a CJK character', '中'],
  ])('never splits %s', (_, char) => {
    const text = `a${char.repeat(50)}`
    const decoder = new TextDecoder('utf-8', { fatal: true })
    for (const max of [4, 5, 6, 7]) {
      const frames = splitFrames(Buffer.from(text), max)
      for (const frame of frames) {
        expect(frame.byteLength).toBeGreaterThan(0)
        expect(frame.byteLength).toBeLessThanOrEqual(max)
      }
      expect(frames.map((frame) => decoder.decode(frame)).join('')).toBe(text)
    }
  })
})

describe('sliceEnd', () => {
  const wrappedAt =
    (...rows: number[]) =>
    (row: number) =>
      rows.includes(row)

  it('ends a slice after the given rows, or at the end', () => {
    expect(sliceEnd(wrappedAt(), 0, 100, 30)).toBe(30)
    expect(sliceEnd(wrappedAt(), 90, 100, 30)).toBe(100)
  })

  it('ends a slice earlier, so a wrapped line stays in one slice', () => {
    expect(sliceEnd(wrappedAt(29, 30, 31), 0, 100, 30)).toBe(28)
  })

  it('goes past the given rows for a wrapped line longer than a slice', () => {
    const rows = Array.from({ length: 40 }, (_, index) => index + 1)
    expect(sliceEnd(wrappedAt(...rows), 0, 100, 30)).toBe(41)
  })
})
