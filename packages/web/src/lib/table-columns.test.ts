import { describe, expect, it } from 'vitest'
import { type ColumnKey, tableLayout } from './table-columns'

const both = { changes: true, pr: true }

function columns(width: number | null, has = both): ColumnKey[] | 'cards' {
  const layout = tableLayout(width, has)
  return layout.kind === 'cards' ? 'cards' : layout.columns
}

describe('tableLayout', () => {
  it('has every column from 1000', () => {
    expect(columns(1000)).toEqual([
      'id',
      'task',
      'state',
      'agent',
      'branch',
      'changes',
      'pr',
      'activity',
    ])
    expect(columns(1920)).toHaveLength(8)
  })

  it('drops agent, branch and pull request from 760 to 999', () => {
    expect(columns(999)).toEqual(['id', 'task', 'state', 'changes', 'activity'])
    expect(columns(760)).toEqual(['id', 'task', 'state', 'changes', 'activity'])
  })

  it('keeps id, task, state and last activity from 560 to 759', () => {
    expect(columns(759)).toEqual(['id', 'task', 'state', 'activity'])
    expect(columns(560)).toEqual(['id', 'task', 'state', 'activity'])
  })

  it('is a card list under 560', () => {
    expect(columns(559)).toBe('cards')
    expect(columns(390)).toBe('cards')
  })

  it('hides changes and pull request when no row has a value', () => {
    expect(columns(1200, { changes: false, pr: true })).not.toContain('changes')
    expect(columns(1200, { changes: true, pr: false })).not.toContain('pr')
    expect(columns(900, { changes: false, pr: false })).toEqual(['id', 'task', 'state', 'activity'])
  })

  it('uses the widest set when the width is unknown', () => {
    expect(columns(null)).toHaveLength(8)
  })

  it('lets the task column take the free width', () => {
    const layout = tableLayout(1200, both)
    expect(layout.kind === 'table' && layout.template).toContain('1fr')
  })
})
