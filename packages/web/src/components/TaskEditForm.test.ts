import { describe, expect, it } from 'vitest'
import { parseLabels } from './TaskEditForm'

describe('parseLabels', () => {
  it('splits on commas, trims, and drops empty and repeated labels', () => {
    expect(parseLabels('ui, parser , ,api,ui')).toEqual(['ui', 'parser', 'api'])
    expect(parseLabels('  ')).toEqual([])
  })
})
