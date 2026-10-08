import { describe, expect, it } from 'vitest'
import { parseLabels, parseTaskIds } from './TaskEditForm'

describe('parseLabels', () => {
  it('splits on commas, trims, and drops empty and repeated labels', () => {
    expect(parseLabels('ui, parser , ,api,ui')).toEqual(['ui', 'parser', 'api'])
    expect(parseLabels('  ')).toEqual([])
  })
})

describe('parseTaskIds', () => {
  it('keeps task ids, upper case, once', () => {
    expect(parseTaskIds('t-1, T-0002 x T-0002,,')).toEqual(['T-1', 'T-0002'])
  })
})
