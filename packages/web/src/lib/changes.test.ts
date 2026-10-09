import { describe, expect, it } from 'vitest'
import {
  countLineChanges,
  countLines,
  stepFile,
  trimTrailingNewline,
  wholeFileKind,
  wrapsByDefault,
} from './changes'

describe('wholeFileKind', () => {
  it('shows new files as added, deleted files as removed, and the rest as a diff', () => {
    expect(['A', 'U', 'D', 'M', 'R', 'C'].map((status) => wholeFileKind(status as 'A'))).toEqual([
      'added',
      'added',
      'removed',
      null,
      null,
      null,
    ])
  })
})

describe('trimTrailingNewline', () => {
  it('drops one final line break, so no empty last line shows', () => {
    expect(trimTrailingNewline('a\nb\n')).toBe('a\nb')
    expect(trimTrailingNewline('a\r\n')).toBe('a')
    expect(trimTrailingNewline('a\n\n')).toBe('a\n')
    expect(trimTrailingNewline('a')).toBe('a')
  })
})

describe('countLines', () => {
  it('counts the lines of a file', () => {
    expect(countLines('')).toBe(0)
    expect(countLines('a\n')).toBe(1)
    expect(countLines('a\nb\nc\n')).toBe(3)
  })
})

describe('countLineChanges', () => {
  it('adds up lines on each side, and treats an end of 0 as no lines', () => {
    expect(
      countLineChanges([
        {
          originalStartLineNumber: 3,
          originalEndLineNumber: 4,
          modifiedStartLineNumber: 3,
          modifiedEndLineNumber: 3,
        },
        {
          originalStartLineNumber: 9,
          originalEndLineNumber: 0,
          modifiedStartLineNumber: 10,
          modifiedEndLineNumber: 12,
        },
      ]),
    ).toEqual({ added: 4, removed: 2 })
    expect(countLineChanges([])).toEqual({ added: 0, removed: 0 })
  })
})

describe('stepFile', () => {
  const paths = ['a', 'b', 'c']

  it('moves to the next and previous file, and stops at the ends', () => {
    expect(stepFile(paths, 'b', 1)).toBe('c')
    expect(stepFile(paths, 'b', -1)).toBe('a')
    expect(stepFile(paths, 'c', 1)).toBe('c')
    expect(stepFile(paths, 'a', -1)).toBe('a')
  })

  it('starts at the first file when none is chosen, and has none for an empty list', () => {
    expect(stepFile(paths, null, 1)).toBe('a')
    expect(stepFile([], null, 1)).toBeNull()
  })
})

describe('wrapsByDefault', () => {
  it('wraps prose and files without an extension, not code', () => {
    expect(wrapsByDefault('docs/notes.md')).toBe(true)
    expect(wrapsByDefault('README.TXT')).toBe(true)
    expect(wrapsByDefault('Makefile')).toBe(true)
    expect(wrapsByDefault('.gitignore')).toBe(true)
    expect(wrapsByDefault('src/a.ts')).toBe(false)
    expect(wrapsByDefault('v1.2/app.json')).toBe(false)
  })
})
