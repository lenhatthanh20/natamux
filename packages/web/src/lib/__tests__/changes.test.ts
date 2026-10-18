import { describe, expect, it } from 'vitest'
import {
  changeAnchor,
  changeAt,
  changeCountLabel,
  countLineChanges,
  countLines,
  type LineChange,
  stepChange,
  stepFile,
  trimTrailingNewline,
  wholeFileKind,
  wholeLines,
  wrapsByDefault,
} from '../changes'

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

// Only the new side matters for navigation.
function change(modifiedStartLineNumber: number, modifiedEndLineNumber: number): LineChange {
  return {
    originalStartLineNumber: 1,
    originalEndLineNumber: 1,
    modifiedStartLineNumber,
    modifiedEndLineNumber,
  }
}

// Lines 3 to 5 changed, then lines were deleted after line 10, then line 20 changed.
const CHANGES = [change(3, 5), change(10, 0), change(20, 20)]

describe('changeAnchor', () => {
  it('covers the new lines of a change', () => {
    expect(changeAnchor(change(3, 5))).toEqual({ start: 3, end: 5 })
    expect(changeAnchor(change(20, 20))).toEqual({ start: 20, end: 20 })
  })

  it('puts a deletion on the line it follows, and a deletion at the top on line 1', () => {
    expect(changeAnchor(change(10, 0))).toEqual({ start: 10, end: 10 })
    expect(changeAnchor(change(0, 0))).toEqual({ start: 1, end: 1 })
  })
})

describe('changeAt', () => {
  it('finds the change that holds a line, and -1 between changes', () => {
    expect(changeAt(CHANGES, 3)).toBe(0)
    expect(changeAt(CHANGES, 4)).toBe(0)
    expect(changeAt(CHANGES, 5)).toBe(0)
    expect(changeAt(CHANGES, 6)).toBe(-1)
    expect(changeAt(CHANGES, 10)).toBe(1)
    expect(changeAt(CHANGES, 11)).toBe(-1)
    expect(changeAt([], 1)).toBe(-1)
  })
})

describe('stepChange', () => {
  it('goes to the first or the last change when no line is known yet', () => {
    expect(stepChange(CHANGES, null, 1)).toBe(0)
    expect(stepChange(CHANGES, null, -1)).toBe(2)
  })

  it('goes to the next change that starts after the line', () => {
    expect(stepChange(CHANGES, 1, 1)).toBe(0)
    expect(stepChange(CHANGES, 3, 1)).toBe(1)
    expect(stepChange(CHANGES, 4, 1)).toBe(1)
    expect(stepChange(CHANGES, 10, 1)).toBe(2)
  })

  it('goes to the previous change that starts before the line', () => {
    expect(stepChange(CHANGES, 20, -1)).toBe(1)
    expect(stepChange(CHANGES, 15, -1)).toBe(1)
    expect(stepChange(CHANGES, 4, -1)).toBe(0)
  })

  it('wraps at both ends', () => {
    expect(stepChange(CHANGES, 20, 1)).toBe(0)
    expect(stepChange(CHANGES, 30, 1)).toBe(0)
    expect(stepChange(CHANGES, 3, -1)).toBe(2)
    expect(stepChange(CHANGES, 1, -1)).toBe(2)
  })

  it('has nothing to go to without changes', () => {
    expect(stepChange([], null, 1)).toBe(-1)
    expect(stepChange([], 5, -1)).toBe(-1)
  })
})

describe('changeCountLabel', () => {
  it('names the change the cursor is in, or counts them all', () => {
    expect(changeCountLabel(1, 7)).toBe('2 of 7')
    expect(changeCountLabel(-1, 7)).toBe('7 changes')
    expect(changeCountLabel(-1, 1)).toBe('1 change')
    expect(changeCountLabel(-1, 0)).toBe('0 changes')
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

describe('wholeLines', () => {
  const range = (startLineNumber: number, endLineNumber: number, endColumn: number) => ({
    startLineNumber,
    endLineNumber,
    endColumn,
  })

  it('covers every line the selection touches', () => {
    expect(wholeLines(range(4, 4, 9))).toEqual({ start: 4, end: 4 })
    expect(wholeLines(range(4, 7, 3))).toEqual({ start: 4, end: 7 })
  })

  it('leaves out a last line that the selection only reaches at column 1', () => {
    expect(wholeLines(range(4, 7, 1))).toEqual({ start: 4, end: 6 })
    expect(wholeLines(range(4, 5, 1))).toEqual({ start: 4, end: 4 })
  })

  it('keeps a one-line selection that starts at column 1', () => {
    expect(wholeLines(range(4, 4, 1))).toEqual({ start: 4, end: 4 })
  })
})
