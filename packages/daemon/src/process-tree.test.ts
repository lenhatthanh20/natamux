import { describe, expect, it } from 'vitest'
import { descendants } from './process-tree.js'

describe('descendants', () => {
  it('finds the processes under a shell, at any depth, and no others', () => {
    const ps = ['  1     0', ' 10     1', ' 11    10', ' 12    11', ' 13    10', ' 20     1', ''].join('\n')
    expect(descendants(ps, 10)).toEqual([11, 12, 13])
    expect(descendants(ps, 12)).toEqual([])
  })
})
