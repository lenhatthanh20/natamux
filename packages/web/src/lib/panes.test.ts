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
import { sortByRecent, upsertTask } from './tasks'

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
    workflowStages: [],
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

describe('agentPaneTasks order', () => {
  const ids = (tasks: readonly Task[]) => tasks.map((task) => task.id)
  // The task list comes newest first, so a task with an event moves to the front.
  const at = (id: string, minute: number, overrides: Partial<Task> = {}) => {
    const updatedAt = `2026-10-08T09:${String(minute).padStart(2, '0')}:00.000Z`
    return makeTask({ id, updatedAt, ...overrides })
  }

  it('orders the default grid by task id, not by the last update', () => {
    expect(ids(agentPaneTasks(null, [at('T-0012', 3), at('T-0003', 2), at('T-0007', 1)]))).toEqual([
      'T-0003',
      'T-0007',
      'T-0012',
    ])
  })

  it('keeps the order when a task gets an update', () => {
    const before = [at('T-0012', 3), at('T-0007', 2), at('T-0003', 1)]
    const after = [at('T-0003', 9, { state: 'needs-input' }), at('T-0012', 3), at('T-0007', 2)]
    expect(ids(agentPaneTasks(null, before))).toEqual(['T-0003', 'T-0007', 'T-0012'])
    expect(ids(agentPaneTasks(null, after))).toEqual(['T-0003', 'T-0007', 'T-0012'])
  })

  it('keeps the others in order when a pane leaves or joins', () => {
    const left = [at('T-0007', 9, { state: 'review' }), at('T-0012', 3), at('T-0003', 1)]
    expect(ids(agentPaneTasks(null, left))).toEqual(['T-0003', 'T-0012'])
    const joined = [at('T-0005', 9), at('T-0012', 3), at('T-0007', 2), at('T-0003', 1)]
    expect(ids(agentPaneTasks(null, joined))).toEqual(['T-0003', 'T-0005', 'T-0007', 'T-0012'])
  })

  it('compares ids by number', () => {
    const tasks = [at('T-10000', 4), at('T-1000', 3), at('T-9999', 2), at('T-0999', 1)]
    expect(ids(agentPaneTasks(null, tasks))).toEqual(['T-0999', 'T-1000', 'T-9999', 'T-10000'])
  })

  it('shows the six lowest ids, whatever gets an update', () => {
    const many = Array.from({ length: 8 }, (_, index) => at(`T-00${index + 10}`, index))
    const lowest = ['T-0010', 'T-0011', 'T-0012', 'T-0013', 'T-0014', 'T-0015']
    expect(ids(agentPaneTasks(null, [...many].reverse()))).toEqual(lowest)
    const updated = [at('T-0017', 30), at('T-0010', 31), ...many.slice(1, 7)]
    expect(ids(agentPaneTasks(null, updated))).toEqual(lowest)
  })

  it('keeps the order the user picked when tasks get updates', () => {
    const tasks = [at('T-0003', 9), at('T-0012', 3), at('T-0007', 2)]
    expect(ids(agentPaneTasks(['T-0012', 'T-0003'], tasks))).toEqual(['T-0012', 'T-0003'])
    expect(ids(agentPaneTasks(['T-0012', 'T-0003', 'T-0007'], tasks))).toEqual([
      'T-0012',
      'T-0003',
      'T-0007',
    ])
    expect(ids(agentPaneTasks(['T-0012', 'T-0007'], tasks))).toEqual(['T-0012', 'T-0007'])
  })
})

describe('agentPaneTasks order with task events', () => {
  const panes = (chosen: readonly string[] | null, tasks: readonly Task[]) =>
    agentPaneTasks(chosen, tasks).map((task) => task.id)
  // Running tasks in the order of the task list. The first id is the newest.
  const newestFirst = (...ids: string[]) =>
    sortByRecent(
      ids.map((id, index) => {
        const updatedAt = new Date(Date.UTC(2026, 9, 8, 9, 0, ids.length - index)).toISOString()
        return makeTask({ id, updatedAt })
      }),
    )
  // A task event as the live feed merges it: newer than every task, so it goes first.
  const event = (tasks: readonly Task[], id: string, overrides: Partial<Task> = {}) => {
    const newest = Math.max(0, ...tasks.map((task) => Date.parse(task.updatedAt)))
    const known = tasks.find((task) => task.id === id) ?? makeTask({ id })
    return upsertTask(tasks, {
      ...known,
      ...overrides,
      updatedAt: new Date(newest + 1000).toISOString(),
    })
  }
  const grid = () => newestFirst('T-0012', 'T-0007', 'T-0003')
  const ids = (first: number, last: number) =>
    Array.from(
      { length: last - first + 1 },
      (_, index) => `T-${String(first + index).padStart(4, '0')}`,
    )

  describe('default grid', () => {
    it('orders panes by task id', () => {
      expect(panes(null, grid())).toEqual(['T-0003', 'T-0007', 'T-0012'])
      expect(panes(null, newestFirst('T-0012'))).toEqual(['T-0012'])
      expect(panes(null, [])).toEqual([])
    })

    it('leaves out tasks without a working agent or a workspace, even with the lowest id', () => {
      let tasks = event(grid(), 'T-0001', { state: 'ready' })
      tasks = event(tasks, 'T-0002', { state: 'review' })
      tasks = event(tasks, 'T-0000', { state: 'done' })
      tasks = event(tasks, 'T-0004', { workspace: null })
      expect(panes(null, tasks)).toEqual(['T-0003', 'T-0007', 'T-0012'])
      expect(
        panes(
          null,
          tasks.filter((task) => task.state === 'ready'),
        ),
      ).toEqual([])
    })

    it('does not move panes on task events', () => {
      let tasks = event(grid(), 'T-0012')
      expect(tasks[0]?.id).toBe('T-0012')
      expect(panes(null, tasks)).toEqual(['T-0003', 'T-0007', 'T-0012'])
      tasks = event(tasks, 'T-0003')
      expect(panes(null, tasks)).toEqual(['T-0003', 'T-0007', 'T-0012'])
      tasks = event(tasks, 'T-0007', { state: 'needs-input' })
      expect(panes(null, tasks)).toEqual(['T-0003', 'T-0007', 'T-0012'])
      for (let count = 0; count < 5; count += 1) {
        tasks = event(tasks, 'T-0012')
      }
      expect(panes(null, tasks)).toEqual(['T-0003', 'T-0007', 'T-0012'])
      tasks = event(event(tasks, 'T-0001', { state: 'ready' }), 'T-0001', { title: 'Renamed' })
      expect(panes(null, tasks)).toEqual(['T-0003', 'T-0007', 'T-0012'])
    })

    it('orders by id when tasks have the same update time', () => {
      const same = ['T-0012', 'T-0003', 'T-0007'].map((id) => makeTask({ id }))
      expect(panes(null, same)).toEqual(['T-0003', 'T-0007', 'T-0012'])
    })

    it('keeps the others in order when a pane leaves', () => {
      expect(panes(null, event(grid(), 'T-0007', { state: 'review' }))).toEqual([
        'T-0003',
        'T-0012',
      ])
      expect(panes(null, event(grid(), 'T-0003', { state: 'done' }))).toEqual(['T-0007', 'T-0012'])
      expect(panes(null, event(grid(), 'T-0012', { workspace: null }))).toEqual([
        'T-0003',
        'T-0007',
      ])
      const all = ['T-0003', 'T-0007', 'T-0012'].reduce(
        (tasks, id) => event(tasks, id, { state: 'review' }),
        grid(),
      )
      expect(panes(null, all)).toEqual([])
    })

    it('puts a pane that joins in its place by id', () => {
      expect(panes(null, event(grid(), 'T-0005'))).toEqual(['T-0003', 'T-0005', 'T-0007', 'T-0012'])
      expect(panes(null, event(grid(), 'T-0001'))).toEqual(['T-0001', 'T-0003', 'T-0007', 'T-0012'])
      expect(panes(null, event(grid(), 'T-0020'))).toEqual(['T-0003', 'T-0007', 'T-0012', 'T-0020'])
      const left = event(grid(), 'T-0007', { state: 'review' })
      expect(panes(null, left)).toEqual(['T-0003', 'T-0012'])
      expect(panes(null, event(left, 'T-0007', { state: 'running' }))).toEqual([
        'T-0003',
        'T-0007',
        'T-0012',
      ])
    })

    it('compares ids by number, not by text', () => {
      // A text sort puts T-10000 before T-9999.
      const tasks = newestFirst('T-1000', 'T-0999', 'T-10000', 'T-9999')
      expect(panes(null, tasks)).toEqual(['T-0999', 'T-1000', 'T-9999', 'T-10000'])
    })

    it('shows the six lowest ids when more agents run, whatever gets an event', () => {
      const shuffled = [
        'T-0005',
        'T-0008',
        'T-0002',
        'T-0007',
        'T-0001',
        'T-0004',
        'T-0006',
        'T-0003',
      ]
      let tasks = newestFirst(...shuffled)
      expect(MAX_PANES).toBe(6)
      expect(panes(null, tasks)).toEqual(ids(1, 6))
      for (const id of ids(1, 8)) {
        tasks = event(tasks, id)
        expect(panes(null, tasks)).toEqual(ids(1, 6))
      }
      expect(panes(null, event(tasks, 'T-0008'))).toEqual(ids(1, 6))
      expect(panes(null, newestFirst(...ids(1, 6).reverse()))).toEqual(ids(1, 6))
      expect(panes(null, event(tasks, 'T-0002', { state: 'review' }))).toEqual([
        'T-0001',
        'T-0003',
        'T-0004',
        'T-0005',
        'T-0006',
        'T-0007',
      ])
    })
  })

  describe('picked grid', () => {
    const tasks = newestFirst('T-0003', 'T-0007', 'T-0012')

    it('keeps the list order on task events, adds last and closes in place', () => {
      // Neither id nor recency order: a sort would fail this.
      const picked = ['T-0012', 'T-0003']
      expect(panes(picked, tasks)).toEqual(picked)
      const later = event(event(tasks, 'T-0003'), 'T-0012')
      expect(panes(picked, event(tasks, 'T-0003'))).toEqual(picked)
      expect(panes(picked, later)).toEqual(picked)
      expect(panes([...picked, 'T-0007'], later)).toEqual(['T-0012', 'T-0003', 'T-0007'])
      expect(panes(['T-0012', 'T-0007'], later)).toEqual(['T-0012', 'T-0007'])
    })

    it('stays empty after the last pane closes, without the default panes', () => {
      expect(panes([], tasks)).toEqual([])
    })
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
