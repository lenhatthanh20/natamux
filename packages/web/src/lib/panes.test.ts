import { type Task, taskSchema } from '@helmsman/core'
import { describe, expect, it } from 'vitest'
import {
  agentPaneTasks,
  columnsFor,
  MAX_PANES,
  maxColumns,
  paneColumns,
  paneSpan,
  paneStateText,
  parsePaneLayout,
} from './panes'

function makeTask(overrides: Partial<Task>): Task {
  return taskSchema.parse({
    id: 'T-0001',
    title: 'Example',
    description: '',
    state: 'running',
    priority: 'normal',
    labels: [],
    agent: null,
    branch: null,
    workspace: '/work/T-0001',
    pr: null,
    prStatus: null,
    parentId: null,
    repo: null,
    dependsOn: [],
    createdAt: '2026-10-08T09:00:00.000Z',
    updatedAt: '2026-10-08T09:00:00.000Z',
    ...overrides,
  })
}

describe('paneColumns', () => {
  it('puts up to three panes side by side, then wraps', () => {
    expect([0, 1, 2, 3, 4, 5, 6].map(paneColumns)).toEqual([1, 1, 2, 3, 2, 3, 3])
  })
})

describe('agentPaneTasks', () => {
  const working = makeTask({ id: 'T-0001' })
  const waiting = makeTask({ id: 'T-0002', state: 'needs-input' })
  const ready = makeTask({ id: 'T-0003', state: 'ready' })
  const noWorkspace = makeTask({ id: 'T-0004', workspace: null })

  it('shows the agents that work or wait until the user picks', () => {
    expect(agentPaneTasks(null, [working, waiting, ready, noWorkspace])).toEqual([working, waiting])
  })

  it('keeps the order the user picked and drops tasks without a workspace', () => {
    expect(agentPaneTasks(['T-0003', 'T-0004', 'T-0001'], [working, ready, noWorkspace])).toEqual([
      ready,
      working,
    ])
  })

  it('shows at most six panes by default', () => {
    const many = Array.from({ length: 8 }, (_, index) =>
      makeTask({ id: `T-00${String(index + 10)}` }),
    )
    expect(agentPaneTasks(null, many)).toHaveLength(MAX_PANES)
  })
})

describe('maxColumns', () => {
  it('fits panes of at least 320px with the gaps', () => {
    expect([300, 647, 648, 1000, 1440, 2000].map(maxColumns)).toEqual([1, 1, 2, 3, 4, 6])
  })

  it('assumes three columns before the grid is measured', () => {
    expect(maxColumns(null)).toBe(3)
    expect(maxColumns(0)).toBe(3)
  })
})

describe('columnsFor', () => {
  it('lets Auto use the width, with balanced rows', () => {
    expect(columnsFor('auto', 2, 1440)).toBe(2)
    expect(columnsFor('auto', 5, 1440)).toBe(3)
    expect(columnsFor('auto', 6, 1440)).toBe(3)
    expect(columnsFor('auto', 1, 1440)).toBe(1)
    expect(columnsFor('auto', 0, 1440)).toBe(1)
  })

  it('gives the columns that were picked, as far as they fit and as there are panes', () => {
    expect(columnsFor('2', 6, 1440)).toBe(2)
    expect(columnsFor('4', 6, 1440)).toBe(4)
    expect(columnsFor('4', 6, 700)).toBe(2)
    expect(columnsFor('4', 3, 1440)).toBe(3)
    expect(columnsFor('1', 3, 1440)).toBe(1)
  })

  it('stays at one column on a phone', () => {
    expect(columnsFor('auto', 4, 358)).toBe(1)
    expect(columnsFor('4', 4, 358)).toBe(1)
  })
})

describe('paneSpan', () => {
  it('stretches the last pane over the rest of its row', () => {
    expect([0, 1, 2, 3, 4].map((index) => paneSpan(index, 5, 3))).toEqual([1, 1, 1, 1, 2])
    expect([0, 1, 2].map((index) => paneSpan(index, 3, 2))).toEqual([1, 1, 2])
  })

  it('leaves a full row alone', () => {
    expect([0, 1, 2, 3].map((index) => paneSpan(index, 4, 2))).toEqual([1, 1, 1, 1])
    expect(paneSpan(0, 1, 1)).toBe(1)
  })
})

describe('parsePaneLayout', () => {
  it('knows the four layouts and falls back to Auto', () => {
    expect(['1', '2', '4', 'auto', '3', null, undefined].map(parsePaneLayout)).toEqual([
      '1',
      '2',
      '4',
      'auto',
      'auto',
      'auto',
      'auto',
    ])
  })
})

describe('paneStateText', () => {
  it('shows what a running agent does, and says plainly that it waits for the user', () => {
    expect(paneStateText('running', 'Edit notes/T-0018.md')).toBe('Running · Edit notes/T-0018.md')
    expect(paneStateText('running', null)).toBe('Running')
    expect(paneStateText('needs-input', 'Edit a.ts')).toBe('Needs your input')
    expect(paneStateText('review', 'Edit a.ts')).toBe('In review')
  })
})
