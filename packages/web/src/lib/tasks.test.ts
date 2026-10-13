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
  applyTaskChanges,
  contextText,
  countByState,
  filterTasks,
  formatAge,
  isQuiet,
  laneSummary,
  needsMeTasks,
  nextSort,
  rowLabel,
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

  it('puts a changed task where a full sort would, and keeps the other tasks as they are', () => {
    const at = (minute: number) => `2026-10-08T09:0${minute}:00.000Z`
    const tasks = sortByRecent([
      makeTask({ id: 'T-0001', updatedAt: at(1) }),
      makeTask({ id: 'T-0002', updatedAt: at(2) }),
      makeTask({ id: 'T-0003', updatedAt: at(3) }),
      makeTask({ id: 'T-0004', updatedAt: at(3) }),
    ])
    for (const change of [
      makeTask({ id: 'T-0001', updatedAt: at(9) }),
      makeTask({ id: 'T-0004', title: 'Same time', updatedAt: at(3) }),
      makeTask({ id: 'T-0005', updatedAt: at(2) }),
      makeTask({ id: 'T-0000', updatedAt: at(0) }),
    ]) {
      const result = upsertTask(tasks, change)
      expect(result).toEqual(
        sortByRecent([...tasks.filter((task) => task.id !== change.id), change]),
      )
      expect(result.find((task) => task.id === 'T-0002')).toBe(tasks[2])
    }
  })

  it('applies live changes in the order they came, deletions too', () => {
    const first = makeTask({ id: 'T-0001', updatedAt: '2026-10-08T09:00:00.000Z' })
    const changed = { ...first, title: 'Changed', updatedAt: '2026-10-08T10:00:00.000Z' }
    const added = makeTask({ id: 'T-0002', updatedAt: '2026-10-08T11:00:00.000Z' })

    const result = applyTaskChanges([first], [changed, added, { deleted: 'T-0001' }])
    expect(result.map((task) => task.id)).toEqual(['T-0002'])

    const readded = applyTaskChanges([first], [{ deleted: 'T-0001' }, changed])
    expect(readded.map((task) => task.title)).toEqual(['Changed'])
  })
})

describe('stage labels', () => {
  it('names the stage of a task that is in progress', () => {
    const task = makeTask({ stage: 'test-spec', state: 'needs-input' })
    expect(stageLabel(task)).toBe('Test spec')
    expect(stageStateLabel(task)).toBe('Test spec · Needs your input')
  })

  it('names the waiting state the same way for screen readers', () => {
    const task = makeTask({ state: 'needs-input', updatedAt: '2026-10-08T09:00:00.000Z' })
    expect(rowLabel(task, new Date('2026-10-08T09:00:00.000Z'))).toContain(', Needs your input, ')
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

  it.each([
    '2026-10-08T09:01:00.000Z',
    '2026-10-08T09:10:00.000Z',
    '2026-10-08T09:11:00.000Z',
    '2026-10-08T15:00:00.000Z',
  ])('is never true while the agent waits for its background work, at %s', (now) => {
    const waiting = { ...task, backgroundSince: task.updatedAt }
    expect(isQuiet(waiting, new Date(now))).toBe(false)
  })
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

  it('leaves out a running task that waits for background work, however long it waits', () => {
    const tasks = [
      makeTask({
        id: 'T-0001',
        state: 'running',
        updatedAt: '2026-10-08T07:00:00.000Z',
        backgroundSince: '2026-10-08T07:00:00.000Z',
      }),
    ]
    expect(needsMeTasks(tasks, now)).toEqual([])
  })

  // The daemon moves the stage of an agent it ends with the workspace back to ready.
  describe('after a workspace is removed', () => {
    const removedAt = '2026-10-08T09:00:00.000Z'
    const removed = makeTask({ id: 'T-0001', state: 'ready', updatedAt: removedAt })
    const quiet = makeTask({
      id: 'T-0002',
      state: 'running',
      workspace: '/work/T-0002',
      updatedAt: '2026-10-08T08:00:00.000Z',
    })

    it.each([
      '2026-10-08T09:10:00.000Z',
      '2026-10-08T09:10:00.001Z',
      '2026-10-08T09:15:00.000Z',
      '2026-10-09T09:00:00.000Z',
    ])('leaves the task out at %s', (time) => {
      expect(needsMeTasks([removed], new Date(time))).toEqual([])
    })

    it('still lists another workspace that is quiet', () => {
      const listed = needsMeTasks([removed, quiet], new Date('2026-10-08T09:15:00.000Z'))
      expect(listed.map((task) => task.id)).toEqual(['T-0002'])
    })
  })

  it.each<TaskState>(['needs-input', 'failed', 'blocked'])(
    'lists a task with no workspace that is %s',
    (state) => {
      expect(needsMeTasks([makeTask({ state })], now)).toHaveLength(1)
    },
  )

  it('lists a running task with no workspace that has been quiet for a long time', () => {
    const task = makeTask({ state: 'running', updatedAt: '2026-10-08T08:00:00.000Z' })
    expect(needsMeTasks([task], now)).toHaveLength(1)
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

  it('sorts a task with a chosen id by the number at the end of its id', () => {
    const mixed = ['T-0010', 'TASK-0005', 'T-0003'].map((id) => makeTask({ id }))
    expect(sortTasks(mixed, { key: 'id', direction: 'asc' }).map((task) => task.id)).toEqual([
      'T-0003',
      'TASK-0005',
      'T-0010',
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

describe('contextText', () => {
  it('names the source when it has a name, then the reference', () => {
    expect(contextText({ source: 'skill', name: 'linear', ref: 'LIN-7' })).toBe(
      'linear (skill) LIN-7',
    )
    expect(contextText({ source: 'mcp', name: 'jira', ref: '' })).toBe('jira (MCP server)')
    expect(contextText({ source: 'skill', ref: 'LIN-7' })).toBe('skill LIN-7')
    expect(contextText({ source: 'github-issue', ref: '#42' })).toBe('GitHub issue #42')
  })

  it('names a skill picked for the task', () => {
    expect(contextText({ source: 'skill', skill: 'jira-ticket', ref: 'ABC-1' })).toBe(
      'jira-ticket (skill) ABC-1',
    )
    expect(contextText({ source: 'skill', name: 'Ticket', skill: 'jira-ticket', ref: '' })).toBe(
      'jira-ticket (skill)',
    )
  })
})
