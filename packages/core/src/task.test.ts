import { describe, expect, it } from 'vitest'
import { taskSchema, upgradeTaskFields } from './task.js'

const minimalTask = {
  id: 'T-42',
  title: 'Fix timezone bug in reminders',
  createdAt: '2026-10-08T09:00:00Z',
  updatedAt: '2026-10-08T09:00:00Z',
}

describe('taskSchema', () => {
  it('fills defaults for a minimal task', () => {
    expect(taskSchema.parse(minimalTask)).toEqual({
      ...minimalTask,
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
      stage: 'implement',
      stages: ['implement', 'review', 'release'],
      lanes: {},
      stageSince: null,
      context: null,
      issue: null,
    })
  })

  it('trims the title', () => {
    expect(taskSchema.parse({ ...minimalTask, title: '  Fix it  ' }).title).toBe('Fix it')
  })

  it.each(['', '   '])('rejects the empty title %j', (title) => {
    expect(taskSchema.safeParse({ ...minimalTask, title }).success).toBe(false)
  })

  it.each(['42', 'T42', 't-42', 'T-', 'T-4a'])('rejects the id %j', (id) => {
    expect(taskSchema.safeParse({ ...minimalTask, id }).success).toBe(false)
  })

  it('rejects an unknown state and priority', () => {
    expect(taskSchema.safeParse({ ...minimalTask, state: 'finished' }).success).toBe(false)
    expect(taskSchema.safeParse({ ...minimalTask, priority: 'asap' }).success).toBe(false)
  })

  it('rejects a dependency that is not a task id', () => {
    expect(taskSchema.safeParse({ ...minimalTask, dependsOn: ['nope'] }).success).toBe(false)
  })

  it('rejects a pr that is not a URL', () => {
    expect(taskSchema.safeParse({ ...minimalTask, pr: 'pull 12' }).success).toBe(false)
  })
})

describe('upgradeTaskFields', () => {
  it.each([
    ['review', 'review'],
    ['done', 'release'],
    ['running', 'implement'],
    ['draft', 'implement'],
  ])('puts an old task in %s at the %s stage', (state, stage) => {
    expect(upgradeTaskFields({ state })).toEqual({
      state,
      stage,
      stages: ['implement', 'review', 'release'],
    })
  })

  it('links the issue an old task was imported from', () => {
    const upgraded = upgradeTaskFields({
      state: 'draft',
      description: 'Body\n\nImported from https://github.com/acme/app/issues/42',
    })
    expect(upgraded.issue).toEqual({
      provider: 'github',
      key: '#42',
      url: 'https://github.com/acme/app/issues/42',
    })
  })

  it('leaves a task that has a stage as it is', () => {
    const task = { state: 'review', stage: 'plan' }
    expect(upgradeTaskFields(task)).toBe(task)
  })
})
