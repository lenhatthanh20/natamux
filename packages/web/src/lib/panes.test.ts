import { type Task, taskSchema } from '@natamux/core'
import { describe, expect, it } from 'vitest'
import {
  agentPaneTasks,
  columnsFor,
  MAX_PANES,
  maxColumns,
  otherTerminalHint,
  paneSpan,
  paneStateText,
  parsePaneLayout,
  terminalState,
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
    stage: 'implement',
    stages: ['implement', 'review', 'release'],
    lanes: {},
    stageSince: null,
    backgroundSince: null,
    context: null,
    issue: null,
    parentId: null,
    repo: null,
    dependsOn: [],
    createdAt: '2026-10-08T09:00:00.000Z',
    updatedAt: '2026-10-08T09:00:00.000Z',
    ...overrides,
  })
}

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

describe('terminalState', () => {
  const group = (lanes: Task['lanes']) => makeTask({ stage: 'build', state: 'review', lanes })

  it('gives each terminal of a group the state of its own lane', () => {
    const task = group({ implement: 'running', 'test-spec': 'review' })
    expect(terminalState(task, { stage: 'implement', state: 'running' })).toBe('running')
    expect(terminalState(task, { stage: 'test-spec', state: 'running' })).toBe('review')
  })

  it('reads the state from the task, which is fresher than the terminal list', () => {
    const task = group({ implement: 'running', 'test-spec': 'done' })
    expect(terminalState(task, { stage: 'test-spec', state: 'review' })).toBe('done')
  })

  it('gives the task state to the stage the task is at', () => {
    const task = makeTask({ stage: 'spec', state: 'needs-input' })
    expect(terminalState(task, { stage: 'spec', state: 'running' })).toBe('needs-input')
  })

  it('keeps what the daemon said for a stage the task has left', () => {
    const task = makeTask({ stage: 'review', state: 'review' })
    expect(terminalState(task, { stage: 'implement', state: 'done' })).toBe('done')
  })

  it('has no state for a terminal that works on no stage', () => {
    expect(terminalState(makeTask({}), { stage: null, state: null })).toBeNull()
  })
})

describe('otherTerminalHint', () => {
  const task = makeTask({
    stage: 'build',
    state: 'needs-input',
    lanes: { implement: 'running', 'test-spec': 'needs-input' },
  })

  it('names another terminal that waits for the user', () => {
    const lanes = {
      '1': { stage: 'implement', state: 'running' as const },
      '2': { stage: 'test-spec', state: 'running' as const },
    }
    expect(otherTerminalHint(task, lanes)).toBe('Shell 2 needs your input')
  })

  it('names a failed terminal, after one that waits', () => {
    const failed = makeTask({
      stage: 'build',
      state: 'failed',
      lanes: { implement: 'running', 'test-spec': 'failed' },
    })
    const lanes = {
      '1': { stage: 'implement', state: 'running' as const },
      '2': { stage: 'test-spec', state: 'failed' as const },
    }
    expect(otherTerminalHint(failed, lanes)).toBe('Shell 2 failed')
  })

  it('says nothing about the shown terminal, or when the others are fine', () => {
    const own = makeTask({
      stage: 'build',
      state: 'needs-input',
      lanes: { implement: 'needs-input', 'test-spec': 'review' },
    })
    const lanes = {
      '1': { stage: 'implement', state: 'needs-input' as const },
      '2': { stage: 'test-spec', state: 'review' as const },
    }
    expect(otherTerminalHint(own, lanes)).toBeUndefined()
    expect(otherTerminalHint(task, undefined)).toBeUndefined()
  })
})
