import { type Task, taskSchema } from '@helmsman/core'
import { describe, expect, it } from 'vitest'
import { sidebarGroups } from './sidebar-groups'

const NOW = new Date('2026-10-08T12:00:00.000Z')

function makeTask(overrides: Partial<Task>): Task {
  return taskSchema.parse({
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
    parentId: null,
    repo: null,
    dependsOn: [],
    createdAt: '2026-10-08T09:00:00.000Z',
    updatedAt: '2026-10-08T11:59:00.000Z',
    ...overrides,
  })
}

describe('sidebarGroups', () => {
  it('puts every task in exactly one place', () => {
    const tasks = [
      makeTask({ id: 'T-0001', state: 'review' }),
      makeTask({ id: 'T-0002', state: 'running' }),
      makeTask({ id: 'T-0003', state: 'ready' }),
      makeTask({ id: 'T-0004', state: 'done' }),
      makeTask({ id: 'T-0005', state: 'cancelled' }),
      makeTask({ id: 'T-0006', state: 'blocked' }),
    ]
    const groups = sidebarGroups(tasks, NOW)

    expect(groups.needsMe.map((task) => task.id)).toEqual(['T-0001', 'T-0006'])
    expect(groups.open.map((group) => group.state)).toEqual(['running', 'ready'])
    expect(groups.finished.map((task) => task.id)).toEqual(['T-0004', 'T-0005'])
  })

  it('moves a quiet running task to the tasks that need the user', () => {
    const quiet = makeTask({ id: 'T-0007', state: 'running', updatedAt: '2026-10-08T11:00:00Z' })
    const groups = sidebarGroups([quiet], NOW)

    expect(groups.needsMe).toEqual([quiet])
    expect(groups.open).toEqual([])
  })

  it('lists the longest wait first', () => {
    const groups = sidebarGroups(
      [
        makeTask({ id: 'T-0001', state: 'needs-input', updatedAt: '2026-10-08T11:50:00Z' }),
        makeTask({ id: 'T-0002', state: 'needs-input', updatedAt: '2026-10-08T11:10:00Z' }),
      ],
      NOW,
    )

    expect(groups.needsMe.map((task) => task.id)).toEqual(['T-0002', 'T-0001'])
  })
})
