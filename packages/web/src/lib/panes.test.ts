import { type Task, taskSchema } from '@helmsman/core'
import { describe, expect, it } from 'vitest'
import { agentPaneTasks, MAX_PANES, paneColumns } from './panes'

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
