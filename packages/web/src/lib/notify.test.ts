import type { Task, TaskState } from '@natamux/core'
import { describe, expect, it } from 'vitest'
import { notificationFor, statesById, tasksThatStartedWaiting } from './notify'

function task(id: string, state: TaskState): Task {
  return {
    id,
    title: `Title of ${id}`,
    description: '',
    state,
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
  }
}

describe('tasksThatStartedWaiting', () => {
  it('lists tasks that moved into a state that needs the user', () => {
    const before = statesById([
      task('T-0001', 'running'),
      task('T-0002', 'running'),
      task('T-0003', 'review'),
      task('T-0004', 'running'),
    ])
    const now = [
      task('T-0001', 'needs-input'),
      task('T-0002', 'running'),
      task('T-0003', 'failed'),
      task('T-0004', 'review'),
      task('T-0005', 'failed'),
    ]

    expect(tasksThatStartedWaiting(before, now).map((entry) => entry.id)).toEqual([
      'T-0001',
      'T-0004',
    ])
  })

  it('lists nothing the first time', () => {
    expect(tasksThatStartedWaiting(new Map(), [task('T-0001', 'review')])).toEqual([])
  })
})

describe('notificationFor', () => {
  it('names the task and its state', () => {
    expect(notificationFor(task('T-0001', 'needs-input'))).toEqual({
      title: 'T-0001: Needs input',
      body: 'Title of T-0001',
    })
  })
})
