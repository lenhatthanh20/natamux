import {
  applyTaskEvent,
  LEGACY_PIPELINE,
  TASK_STATES,
  type Task,
  type TaskState,
} from '@natamux/core'
import { describe, expect, it } from 'vitest'
import {
  actionsFor,
  agentAction,
  countByState,
  filterTasks,
  formatAge,
  isQuiet,
  laneSummary,
  needsMeReason,
  needsMeTasks,
  nextSort,
  STATE_LABEL,
  sortByRecent,
  sortTasks,
  stageLabel,
  stageStateLabel,
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
    prStatus: null,
    stage: 'implement',
    stages: ['implement', 'review', 'release'],
    lanes: {},
    stageSince: null,
    context: null,
    issue: null,
    parentId: null,
    repo: null,
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
        const result = applyTaskEvent(
          { stage: state === 'review' ? 'review' : 'implement', state, lanes: {} },
          {
            taskId: 'T-0001',
            source: 'human',
            type: action.type,
            at: '2026-10-08T09:00:00.000Z',
            payload: {},
          },
          LEGACY_PIPELINE,
        )
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

  it('keeps the task that was updated last', () => {
    const live = makeTask({ title: 'Live', updatedAt: '2026-10-08T10:00:00.000Z' })
    const late = makeTask({ title: 'Late answer', updatedAt: '2026-10-08T09:00:00.000Z' })
    expect(upsertTask([live], late).map((task) => task.title)).toEqual(['Live'])
    expect(upsertTask([late], live).map((task) => task.title)).toEqual(['Live'])
  })

  it('adds a task that is new', () => {
    const result = upsertTask([makeTask({ id: 'T-0001' })], makeTask({ id: 'T-0002' }))
    expect(result).toHaveLength(2)
  })
})

describe('stage labels', () => {
  it('names the stage of a task that is in progress', () => {
    const task = makeTask({ stage: 'test-spec', state: 'needs-input' })
    expect(stageLabel(task)).toBe('Test spec')
    expect(stageStateLabel(task)).toBe('Test spec · Needs input')
  })

  it('has no stage for a draft or a finished task', () => {
    for (const state of ['draft', 'done', 'cancelled'] as const) {
      const task = makeTask({ stage: 'implement', state })
      expect(stageLabel(task)).toBeNull()
      expect(stageStateLabel(task)).toBe(STATE_LABEL[state])
    }
  })
})

describe('laneSummary', () => {
  it('lists each stage of a group with its state', () => {
    const task = makeTask({
      stage: 'build',
      state: 'review',
      lanes: { 'test-spec': 'review', implement: 'running' },
    })
    expect(laneSummary(task)).toBe('Test spec: In review · Implement: Running')
  })

  it('is empty outside a group', () => {
    expect(laneSummary(makeTask({ state: 'running' }))).toBeUndefined()
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
    makeTask({ id: 'T-0002', title: 'Add export', state: 'review', branch: 'natamux/export' }),
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
    ['natamux/export', ['T-0002']],
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
    ['2026-10-08T11:59:30.000Z', 'now', 'just now'],
    ['2026-10-08T12:00:30.000Z', 'now', 'just now'],
    ['2026-10-08T11:58:30.000Z', '1m', '1m ago'],
    ['2026-10-08T11:00:01.000Z', '59m', '59m ago'],
    ['2026-10-08T11:00:00.000Z', '1h', '1h ago'],
    ['2026-10-07T13:00:00.000Z', '23h', '23h ago'],
    ['2026-10-05T12:00:00.000Z', '3d', '3d ago'],
    ['2026-10-02T12:00:00.000Z', '6d', '6d ago'],
    ['2026-10-01T12:00:00.000Z', 'Oct 1', 'Oct 1'],
    ['2025-10-03T12:00:00.000Z', 'Oct 3, 2025', 'Oct 3, 2025'],
  ])('shows %s as %s (short) and %s (long)', (isoTime, short, long) => {
    expect(formatAge(isoTime, now, 'short')).toBe(short)
    expect(formatAge(isoTime, now, 'long')).toBe(long)
  })

  it('is short by default', () => {
    expect(formatAge('2026-10-08T11:58:30.000Z', now)).toBe('1m')
  })
})

describe('sortTasks', () => {
  const tasks = [
    makeTask({ id: 'T-0001', title: 'beta', state: 'done', agent: 'claude-code' }),
    makeTask({ id: 'T-0002', title: 'Alpha', state: 'running', branch: 'natamux/b' }),
    makeTask({ id: 'T-0010', title: 'gamma', state: 'running', branch: 'natamux/a' }),
    makeTask({ id: 'T-0003', title: 'delta', state: 'needs-input' }),
    makeTask({ id: 'T-0004', title: 'eps', state: 'review', pr: 'https://github.com/o/r/pull/12' }),
    makeTask({ id: 'T-0005', title: 'zeta', state: 'review', pr: 'https://github.com/o/r/pull/9' }),
  ]
  const ids = (sort: Parameters<typeof sortTasks>[1], stats = {}) =>
    sortTasks(tasks, sort, stats).map((task) => task.id)

  it('puts the states that need the user first, then the newest id first', () => {
    expect(ids(null)).toEqual(['T-0003', 'T-0005', 'T-0004', 'T-0010', 'T-0002', 'T-0001'])
  })

  it('sorts ids as numbers, not as text', () => {
    expect(ids({ key: 'id', direction: 'asc' })[5]).toBe('T-0010')
    expect(ids({ key: 'id', direction: 'desc' })[0]).toBe('T-0010')
  })

  it('sorts titles without regard to case', () => {
    expect(ids({ key: 'task', direction: 'asc' }).slice(0, 2)).toEqual(['T-0002', 'T-0001'])
  })

  it('sorts pull requests as numbers and keeps tasks without one last in both directions', () => {
    expect(ids({ key: 'pr', direction: 'asc' }).slice(0, 2)).toEqual(['T-0005', 'T-0004'])
    expect(ids({ key: 'pr', direction: 'desc' }).slice(0, 2)).toEqual(['T-0004', 'T-0005'])
    expect(ids({ key: 'pr', direction: 'desc' }).slice(2)).toEqual(
      ids(null).filter((id) => !['T-0004', 'T-0005'].includes(id)),
    )
  })

  it('breaks ties with the default order, so equal rows keep their place', () => {
    expect(ids({ key: 'branch', direction: 'asc' }).slice(0, 2)).toEqual(['T-0010', 'T-0002'])
    expect(ids({ key: 'state', direction: 'asc' })).toEqual(ids(null))
  })

  it('sorts by the size of the work and puts tasks with no changes last', () => {
    const stats = {
      'T-0001': { files: 1, added: 50, removed: 0 },
      'T-0002': { files: 2, added: 3, removed: 2 },
      'T-0003': { files: 0, added: 0, removed: 0 },
    }
    expect(ids({ key: 'changes', direction: 'desc' }, stats).slice(0, 2)).toEqual([
      'T-0001',
      'T-0002',
    ])
    expect(ids({ key: 'changes', direction: 'asc' }, stats).slice(0, 2)).toEqual([
      'T-0002',
      'T-0001',
    ])
  })

  it('does not change the list it is given', () => {
    const before = tasks.map((task) => task.id)
    sortTasks(tasks, { key: 'id', direction: 'asc' })
    expect(tasks.map((task) => task.id)).toEqual(before)
  })
})

describe('nextSort', () => {
  it('goes ascending, descending, then back to the default order', () => {
    const first = nextSort(null, 'task')
    expect(first).toEqual({ key: 'task', direction: 'asc' })
    const second = nextSort(first, 'task')
    expect(second).toEqual({ key: 'task', direction: 'desc' })
    expect(nextSort(second, 'task')).toBeNull()
  })

  it('starts ascending on another column', () => {
    expect(nextSort({ key: 'task', direction: 'desc' }, 'id')).toEqual({
      key: 'id',
      direction: 'asc',
    })
  })
})

describe('needsMeReason', () => {
  const now = new Date('2026-10-08T12:00:00.000Z')

  it.each<[TaskState, string]>([
    ['needs-input', 'asks'],
    ['review', 'review'],
    ['failed', 'failed'],
    ['blocked', 'blocked'],
  ])('says %s as %s', (state, reason) => {
    expect(needsMeReason(makeTask({ state }), now)).toBe(reason)
  })

  it('says quiet for a running task that went silent', () => {
    expect(
      needsMeReason(makeTask({ state: 'running', updatedAt: '2026-10-08T11:00:00.000Z' }), now),
    ).toBe('quiet')
  })
})

describe('agentAction', () => {
  it('starts an agent when none was started', () => {
    expect(agentAction({ state: 'ready', agent: null })).toBe('start')
    expect(agentAction({ state: 'draft', agent: null })).toBe('start')
  })

  it('has nothing to offer while the agent works or waits', () => {
    expect(agentAction({ state: 'running', agent: 'claude-code' })).toBeNull()
    expect(agentAction({ state: 'needs-input', agent: 'claude-code' })).toBeNull()
  })

  it('restarts an agent that ended or failed', () => {
    expect(agentAction({ state: 'failed', agent: 'claude-code' })).toBe('restart')
    expect(agentAction({ state: 'review', agent: 'claude-code' })).toBe('restart')
  })
})
