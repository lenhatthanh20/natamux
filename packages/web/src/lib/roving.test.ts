import { describe, expect, it } from 'vitest'
import { firstChildKey, moveTarget, parentKey, typeAheadTarget } from './roving'

describe('moveTarget', () => {
  it('moves one step and stops at the ends', () => {
    expect(moveTarget('ArrowDown', 0, 3)).toBe(1)
    expect(moveTarget('ArrowDown', 2, 3)).toBe(2)
    expect(moveTarget('ArrowUp', 1, 3)).toBe(0)
    expect(moveTarget('ArrowUp', 0, 3)).toBe(0)
  })

  it('jumps to the first and the last item', () => {
    expect(moveTarget('Home', 2, 5)).toBe(0)
    expect(moveTarget('End', 2, 5)).toBe(4)
  })

  it('uses j and k only when asked', () => {
    expect(moveTarget('j', 0, 3)).toBeNull()
    expect(moveTarget('j', 0, 3, true)).toBe(1)
    expect(moveTarget('k', 1, 3, true)).toBe(0)
  })

  it('ignores other keys and empty lists', () => {
    expect(moveTarget('a', 0, 3)).toBeNull()
    expect(moveTarget('ArrowDown', 0, 0)).toBeNull()
  })
})

describe('typeAheadTarget', () => {
  const labels = ['Fix login', 'Add export', 'Add import', 'Zoom']

  it('finds the first label that starts with the text, after the current one', () => {
    expect(typeAheadTarget(labels, 'a', 0)).toBe(1)
  })

  it('goes to the next match when the same letter is pressed again', () => {
    expect(typeAheadTarget(labels, 'a', 1)).toBe(2)
    expect(typeAheadTarget(labels, 'a', 2)).toBe(1)
  })

  it('keeps the current item while a longer word still matches it', () => {
    expect(typeAheadTarget(labels, 'add i', 1)).toBe(2)
    expect(typeAheadTarget(labels, 'add e', 1)).toBe(1)
  })

  it('returns -1 when nothing matches', () => {
    expect(typeAheadTarget(labels, 'q', 0)).toBe(-1)
    expect(typeAheadTarget([], 'a', 0)).toBe(-1)
  })
})

describe('tree rows', () => {
  const rows = [
    { key: 'repo:a', parent: null, expandable: true },
    { key: 'task:1', parent: 'repo:a', expandable: false },
    { key: 'task:2', parent: 'repo:a', expandable: false },
    { key: 'repo:b', parent: null, expandable: true },
  ]

  it('finds the parent of a row', () => {
    expect(parentKey(rows, 'task:2')).toBe('repo:a')
    expect(parentKey(rows, 'repo:a')).toBeNull()
  })

  it('finds the first child of a group, or none for an empty one', () => {
    expect(firstChildKey(rows, 'repo:a')).toBe('task:1')
    expect(firstChildKey(rows, 'repo:b')).toBeNull()
  })
})
