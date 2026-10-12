import { type Task, taskSchema } from '@natamux/core'
import { describe, expect, it } from 'vitest'
import {
  agentChildren,
  isFoldOpen,
  opensByDefault,
  parseFolds,
  stagePosition,
  subtaskCount,
  urgentState,
  withFold,
  withParents,
} from './sidebar-tree'

function makeTask(overrides: Partial<Task>): Task {
  return taskSchema.parse({
    id: 'T-0001',
    title: 'Example',
    state: 'running',
    workspace: '/work/T-0001',
    createdAt: '2026-10-08T09:00:00.000Z',
    updatedAt: '2026-10-08T09:00:00.000Z',
    ...overrides,
  })
}

describe('folds', () => {
  it('reads saved folds and drops anything that is not true or false', () => {
    expect(parseFolds('{"repo:a":false,"task:T-1":true,"bad":"yes"}')).toEqual({
      'repo:a': false,
      'task:T-1': true,
    })
    expect(parseFolds('[1]')).toEqual({})
    expect(parseFolds('not json')).toEqual({})
    expect(parseFolds(null)).toEqual({})
  })

  it('uses the default until the user picks', () => {
    expect(isFoldOpen({}, 'task:T-1', true)).toBe(true)
    expect(isFoldOpen({ 'task:T-1': false }, 'task:T-1', true)).toBe(false)
  })

  it('keeps only the newest choices', () => {
    let folds: Record<string, boolean> = {}
    for (let index = 0; index < 305; index += 1) {
      folds = withFold(folds, `task:T-${index}`, true)
    }
    expect(Object.keys(folds)).toHaveLength(300)
    expect(folds['task:T-0']).toBeUndefined()
    expect(folds['task:T-304']).toBe(true)
  })
})

describe('urgentState', () => {
  it('picks the state that needs the user most', () => {
    expect(urgentState(['running', 'failed', 'needs-input'])).toBe('needs-input')
    expect(urgentState(['ready', 'running', 'failed'])).toBe('failed')
    expect(urgentState(['ready', 'review', 'blocked'])).toBe('blocked')
    expect(urgentState(['ready', 'review'])).toBe('review')
    expect(urgentState(['ready', 'running'])).toBe('running')
    expect(urgentState(['done', 'ready'])).toBe('ready')
  })

  it('is null when nothing is open', () => {
    expect(urgentState(['done', 'cancelled', 'draft'])).toBeNull()
    expect(urgentState([])).toBeNull()
  })
})

describe('opensByDefault', () => {
  it('opens when a child works, waits or failed', () => {
    expect(opensByDefault(['ready', 'running'])).toBe(true)
    expect(opensByDefault(['needs-input'])).toBe(true)
    expect(opensByDefault(['failed'])).toBe(true)
    expect(opensByDefault(['ready', 'review', 'done'])).toBe(false)
  })
})

describe('stagePosition', () => {
  it('counts the current step from the stages of the task', () => {
    const task = makeTask({ stage: 'review', stages: ['spec', 'implement', 'review', 'release'] })
    expect(stagePosition(task)).toEqual({ at: 3, of: 4 })
  })

  it('counts a group of parallel stages as one step, by its id or one of its stages', () => {
    const stages = ['spec', { id: 'checks', parallel: ['lint', 'security'] }, 'release']
    expect(stagePosition(makeTask({ stage: 'checks', stages }))).toEqual({ at: 2, of: 3 })
    expect(stagePosition(makeTask({ stage: 'security', stages }))).toEqual({ at: 2, of: 3 })
  })

  it('is null for one step, an unknown stage, or a task with no stage to show', () => {
    expect(stagePosition(makeTask({ stage: 'implement', stages: ['implement'] }))).toBeNull()
    expect(stagePosition(makeTask({ stage: 'other', stages: ['spec', 'implement'] }))).toBeNull()
    const stages = ['spec', 'implement']
    expect(stagePosition(makeTask({ state: 'done', stage: 'implement', stages }))).toBeNull()
    expect(stagePosition(makeTask({ state: 'draft', stage: 'spec', stages }))).toBeNull()
  })
})

describe('subtaskCount', () => {
  it('counts every subtask of the task, and the done ones', () => {
    const tasks = [
      makeTask({ id: 'T-0001' }),
      makeTask({ id: 'T-0002', parentId: 'T-0001', state: 'done' }),
      makeTask({ id: 'T-0003', parentId: 'T-0001', workspace: null, state: 'ready' }),
      makeTask({ id: 'T-0004', parentId: 'T-0009' }),
    ]
    expect(subtaskCount('T-0001', tasks)).toEqual({ done: 1, total: 2 })
    expect(subtaskCount('T-0004', tasks)).toBeNull()
  })
})

describe('withParents', () => {
  it('brings back the parent of a subtask that the filter matched', () => {
    const parent = makeTask({ id: 'T-0001', title: 'Parent' })
    const child = makeTask({ id: 'T-0002', title: 'Child', parentId: 'T-0001' })
    const other = makeTask({ id: 'T-0003', title: 'Other' })
    const all = [parent, child, other]
    expect(withParents([child], all).map((task) => task.id)).toEqual(['T-0002', 'T-0001'])
    expect(withParents([parent, child], all).map((task) => task.id)).toEqual(['T-0001', 'T-0002'])
    expect(withParents([other], all)).toEqual([other])
  })

  it('does not bring back a parent without a workspace', () => {
    const parent = makeTask({ id: 'T-0001', workspace: null })
    const child = makeTask({ id: 'T-0002', parentId: 'T-0001' })
    expect(withParents([child], [parent, child])).toEqual([child])
  })
})

describe('agentChildren', () => {
  const task = makeTask({
    stage: 'checks',
    state: 'needs-input',
    lanes: { alpha: 'running', beta: 'needs-input' },
  })

  it('lists one row per parallel stage before the terminals are known', () => {
    expect(agentChildren(task)).toEqual([
      { key: 'lane:alpha', name: 'Alpha', stage: null, state: 'running', shell: null },
      { key: 'lane:beta', name: 'Beta', stage: null, state: 'needs-input', shell: null },
    ])
  })

  it('names each agent terminal, with its stage and state, in shell order', () => {
    const rows = agentChildren(task, {
      '10': { stage: null, state: null },
      '2': { stage: 'beta', state: null },
      '1': { stage: 'alpha', state: null },
    })
    expect(rows).toEqual([
      { key: 'shell:1', name: 'Shell 1', stage: 'Alpha', state: 'running', shell: '1' },
      { key: 'shell:2', name: 'Shell 2', stage: 'Beta', state: 'needs-input', shell: '2' },
      { key: 'shell:10', name: 'Shell 10', stage: null, state: 'needs-input', shell: '10' },
    ])
  })

  it('keeps a parallel stage that no terminal works on', () => {
    const rows = agentChildren(task, { '1': { stage: 'alpha', state: null } })
    expect(rows.map((row) => row.key)).toEqual(['shell:1', 'lane:beta'])
  })
})
