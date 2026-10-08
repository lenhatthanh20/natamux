import { describe, expect, it } from 'vitest'
import { freezeOrder } from './frozen-order'

const item = (id: string, value = 0) => ({ id, value })

describe('freezeOrder', () => {
  it('keeps the old order when the new order differs', () => {
    const result = freezeOrder(['a', 'b', 'c'], [item('c'), item('a'), item('b')])
    expect(result.map((entry) => entry.id)).toEqual(['a', 'b', 'c'])
  })

  it('shows the new values in the old places', () => {
    const result = freezeOrder(['a', 'b'], [item('b', 2), item('a', 1)])
    expect(result).toEqual([item('a', 1), item('b', 2)])
  })

  it('drops items that are gone and adds new ones last', () => {
    const result = freezeOrder(['a', 'b', 'c'], [item('d'), item('c'), item('a')])
    expect(result.map((entry) => entry.id)).toEqual(['a', 'c', 'd'])
  })

  it('starts from the given order when nothing was seen before', () => {
    const result = freezeOrder([], [item('b'), item('a')])
    expect(result.map((entry) => entry.id)).toEqual(['b', 'a'])
  })
})
