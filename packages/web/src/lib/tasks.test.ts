import { applyTaskEvent, TASK_STATES, type Task, type TaskState } from '@helmsman/core'
import { describe, expect, it } from 'vitest'
import {
  actionsFor,
  countByState,
  filterTasks,
  formatAge,
  isQuiet,
  needsMeTasks,
  sortByRecent,
  upsertTask,
} from './tasks'

function makeTask(overrides: Partial<Task> = {}): Task {
  return {
    id: 'T-0001',
    title: 'Example',
    description: '',
    state: 'draft',
    priority: 'normal',
    labels: [],
    agent: null,
    branch: null,
    workspace: null,
    pr: null,
    dependsOn: [],
    createdAt: '2026-10-08T09:00:00.000Z',
    updatedAt: '2026-10-08T09:00:00.000Z',
    ...overrides,
  }
}

describe('actionsFor', () => {
  it('offers approve and request changes for a task in review', () => {
    const actions = actionsFor('review')
    expect(actions.map((action) => action.label)).toEqual([
      'Approve',
      'Request changes',
      'Cancel task',
    ])
    expect(actions.map((action) => action.hotkey)).toEqual(['a', 'r', undefined])
  })

  it('offers nothing for a finished task', () => {
    expect(actionsFor('done')).toEqual([])
    expect(actionsFor('cancelled')).toEqual([])
  })

  it('asks to confirm only the cancel action', () => {
    for (const state of TASK_STATES) {
      for (const action of actionsFor(state)) {
        expect(action.confirm === true).toBe(action.type === 'task.cancelled')
      }
    }
  })

  it('only offers events the daemon accepts for that state', () => {
    for (const state of TASK_STATES) {
      for (const action of actionsFor(state)) {
        const result = applyTaskEvent(state, {
          taskId: 'T-0001',
          source: 'human',
          type: action.type,
          at: '2026-10-08T09:00:00.000Z',
          payload: {},
        })
        expect(result.accepted, `${action.label} in ${state}`).toBe(true)
      }
    }
  })
})

describe('sorting and merging', () => {
  it('puts the most recently updated task first', () => {
    const older = makeTask({ id: 'T-0001', updatedAt: '2026-10-08T09:00:00.000Z' })
    const newer = makeTask({ id: 'T-0002', updatedAt: '2026-10-08T10:00:00.000Z' })
    expect(sortByRecent([older, newer]).map((task) => task.id)).toEqual(['T-0002', 'T-0001'])
  })

  it('replaces a task that is already in the list', () => {
    const before = makeTask({ title: 'Old' })
    const result = upsertTask([before], makeTask({ title: 'New' }))
    expect(result.map((task) => task.title)).toEqual(['New'])
  })

  it('adds a task that is new', () => {
    const result = upsertTask([makeTask({ id: 'T-0001' })], makeTask({ id: 'T-0002' }))
    expect(result).toHaveLength(2)
  })
})

describe('isQuiet', () => {
  const task = makeTask({ state: 'running', updatedAt: '2026-10-08T09:00:00.000Z' })

  it('is false while the last sign of life is recent', () => {
    expect(isQuiet(task, new Date('2026-10-08T09:09:59.000Z'))).toBe(false)
  })

  it('is true once the quiet time has passed', () => {
    expect(isQuiet(task, new Date('2026-10-08T09:10:00.000Z'))).toBe(true)
    expect(isQuiet(task, new Date('2026-10-08T11:00:00.000Z'))).toBe(true)
  })

  it.each<TaskState>(['draft', 'ready', 'needs-input', 'review', 'blocked', 'failed', 'done'])(
    'is never true for a task that is %s',
    (state) => {
      expect(isQuiet({ ...task, state }, new Date('2026-10-09T09:00:00.000Z'))).toBe(false)
    },
  )
})

describe('needsMeTasks', () => {
  const now = new Date('2026-10-08T09:05:00.000Z')

  it('keeps only tasks that need the user, longest wait first', () => {
    const tasks = [
      makeTask({ id: 'T-0001', state: 'running' }),
      makeTask({ id: 'T-0002', state: 'review', updatedAt: '2026-10-08T10:00:00.000Z' }),
      makeTask({ id: 'T-0003', state: 'failed', updatedAt: '2026-10-08T08:00:00.000Z' }),
      makeTask({ id: 'T-0004', state: 'done' }),
    ]
    expect(needsMeTasks(tasks, now).map((task) => task.id)).toEqual(['T-0003', 'T-0002'])
  })

  it('adds a running task that has been quiet for a long time', () => {
    const tasks = [
      makeTask({ id: 'T-0001', state: 'running', updatedAt: '2026-10-08T08:30:00.000Z' }),
      makeTask({ id: 'T-0002', state: 'running', updatedAt: '2026-10-08T09:04:00.000Z' }),
      makeTask({ id: 'T-0003', state: 'review', updatedAt: '2026-10-08T09:00:00.000Z' }),
    ]
    expect(needsMeTasks(tasks, now).map((task) => task.id)).toEqual(['T-0001', 'T-0003'])
  })

  it('does not change the list it is given', () => {
    const tasks = [makeTask({ id: 'T-0002', state: 'review' }), makeTask({ id: 'T-0001' })]
    needsMeTasks(tasks, now)
    expect(tasks.map((task) => task.id)).toEqual(['T-0002', 'T-0001'])
  })
})

describe('countByState', () => {
  it('counts every state, with zero for the empty ones', () => {
    const counts = countByState([
      makeTask({ id: 'T-0001', state: 'running' }),
      makeTask({ id: 'T-0002', state: 'running' }),
      makeTask({ id: 'T-0003', state: 'review' }),
    ])
    expect(counts.running).toBe(2)
    expect(counts.review).toBe(1)
    expect(counts.done).toBe(0)
    expect(Object.keys(counts)).toEqual([...TASK_STATES])
  })
})

describe('filterTasks', () => {
  const tasks = [
    makeTask({ id: 'T-0001', title: 'Fix timezone', state: 'running', agent: 'claude-code' }),
    makeTask({ id: 'T-0002', title: 'Add export', state: 'review', branch: 'helmsman/export' }),
    makeTask({ id: 'T-0003', title: 'Cleanup', state: 'review', labels: ['chore'] }),
  ]

  function ids(state: TaskState | null, search: string): string[] {
    return filterTasks(tasks, state, search).map((task) => task.id)
  }

  it('returns everything without a filter', () => {
    expect(ids(null, '')).toEqual(['T-0001', 'T-0002', 'T-0003'])
  })

  it('filters by state', () => {
    expect(ids('review', '')).toEqual(['T-0002', 'T-0003'])
  })

  it.each<[string, string[]]>([
    ['timezone', ['T-0001']],
    ['T-0003', ['T-0003']],
    ['CLAUDE', ['T-0001']],
    ['helmsman/export', ['T-0002']],
    ['chore', ['T-0003']],
    ['nothing like this', []],
  ])('searches %j in title, id, agent, branch and labels', (search, expected) => {
    expect(ids(null, search)).toEqual(expected)
  })

  it('combines the state and the search', () => {
    expect(ids('review', 'export')).toEqual(['T-0002'])
  })
})

describe('formatAge', () => {
  const now = new Date('2026-10-08T12:00:00.000Z')

  it.each([
    ['2026-10-08T11:59:30.000Z', 'just now'],
    ['2026-10-08T12:00:30.000Z', 'just now'],
    ['2026-10-08T11:58:30.000Z', '1m ago'],
    ['2026-10-08T11:00:01.000Z', '59m ago'],
    ['2026-10-08T11:00:00.000Z', '1h ago'],
    ['2026-10-07T13:00:00.000Z', '23h ago'],
    ['2026-10-05T12:00:00.000Z', '3d ago'],
  ])('shows %s as %s', (isoTime, expected) => {
    expect(formatAge(isoTime, now)).toBe(expected)
  })
})
