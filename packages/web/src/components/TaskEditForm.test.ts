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

  it('keeps custom task ids', () => {
    expect(parseTaskIds('task-5 ABC-12 a1-2')).toEqual(['TASK-5', 'ABC-12'])
    expect(parseTaskIds('t-3, task-0005 ABC-12')).toEqual(['T-3', 'TASK-0005', 'ABC-12'])
    expect(parseTaskIds('a1-2, foo, T-, T-7')).toEqual(['T-7'])
  })
})
