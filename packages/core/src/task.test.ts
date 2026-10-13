import { describe, expect, it } from 'vitest'
import { taskContextSchema, taskSchema, upgradeTaskFields } from './task.js'

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
      backgroundSince: null,
      workflowStages: [],
      context: null,
      issue: null,
    })
  })

  it('reads the workflow stages, and a bad value as none, so a hand-edited file still loads', () => {
    expect(taskSchema.parse({ ...minimalTask, workflowStages: ['plan'] }).workflowStages).toEqual([
      'plan',
    ])
    for (const workflowStages of [null, 'plan', 3, ['Not An Id']]) {
      expect(taskSchema.parse({ ...minimalTask, workflowStages }).workflowStages).toEqual([])
    }
  })

  it('reads the stages checked on CI, and a bad value as none', () => {
    expect(taskSchema.parse({ ...minimalTask, ciStages: ['implement'] }).ciStages).toEqual([
      'implement',
    ])
    expect(taskSchema.parse(minimalTask).ciStages).toBeUndefined()
    expect(taskSchema.parse({ ...minimalTask, ciStages: 'implement' }).ciStages).toBeUndefined()
  })

  it('reads the stages marked to skip, and a bad value as none', () => {
    expect(taskSchema.parse({ ...minimalTask, skipStages: ['testing'] }).skipStages).toEqual([
      'testing',
    ])
    expect(taskSchema.parse(minimalTask).skipStages).toBeUndefined()
    expect(taskSchema.parse({ ...minimalTask, skipStages: 'testing' }).skipStages).toBeUndefined()
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

  it('keeps the name of the context source', () => {
    const context = { source: 'skill', name: 'Linear', ref: 'LIN-7' }
    expect(taskSchema.parse({ ...minimalTask, context }).context).toEqual(context)
  })

  it('keeps the skill picked for the task, and parses a context without one', () => {
    const picked = { source: 'skill', skill: 'jira-ticket', ref: 'ABC-1' }
    expect(taskSchema.parse({ ...minimalTask, context: picked }).context).toEqual(picked)
    const plain = { source: 'skill', name: 'Ticket', ref: 'ABC-1' }
    expect(taskSchema.parse({ ...minimalTask, context: plain }).context).toEqual(plain)
    const empty = { source: 'skill', skill: ' ', ref: 'ABC-1' }
    expect(taskSchema.safeParse({ ...minimalTask, context: empty }).success).toBe(false)
  })

  it('trims the picked skill, and takes 1 to 200 characters of text', () => {
    const parse = (skill: unknown) =>
      taskContextSchema.safeParse({ source: 'skill', skill, ref: 'REC-1' })
    expect(parse('  ticket-reader  ').data?.skill).toBe('ticket-reader')
    expect(parse('a'.repeat(200)).success).toBe(true)
    for (const skill of ['', '   ', 'a'.repeat(201), 42, null]) {
      expect(parse(skill).success).toBe(false)
    }
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
