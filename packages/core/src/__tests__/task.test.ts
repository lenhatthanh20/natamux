import { describe, expect, it } from 'vitest'
import {
  issueLinkSchema,
  splitTaskId,
  taskContextSchema,
  taskSchema,
  upgradeTaskFields,
} from '../task.js'

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
      paths: {},
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

  it('reads whether the task is left to a person, and a bad value as not', () => {
    expect(taskSchema.parse({ ...minimalTask, withoutAgent: true }).withoutAgent).toBe(true)
    expect(taskSchema.parse(minimalTask).withoutAgent).toBeUndefined()
    for (const withoutAgent of [false, 'x', 1]) {
      expect(taskSchema.parse({ ...minimalTask, withoutAgent }).withoutAgent).toBeUndefined()
    }
  })

  it('reads the path taken at each branch, and the paths of a branch in its steps', () => {
    const stages = [
      'plan',
      {
        id: 'after-plan',
        paths: [
          { id: 'small', stages: ['implement', 'testing'] },
          { id: 'pr-review', stages: ['review'] },
        ],
      },
      'release',
    ]
    const task = taskSchema.parse({ ...minimalTask, stages, paths: { 'after-plan': 'small' } })
    expect(task.stages).toEqual(stages)
    expect(task.paths).toEqual({ 'after-plan': 'small' })
    expect(taskSchema.parse({ ...minimalTask, stages }).paths).toEqual({})
  })

  it('trims the title', () => {
    expect(taskSchema.parse({ ...minimalTask, title: '  Fix it  ' }).title).toBe('Fix it')
  })

  it.each(['', '   '])('rejects the empty title %j', (title) => {
    expect(taskSchema.safeParse({ ...minimalTask, title }).success).toBe(false)
  })

  it.each(['42', 'T42', 't-42', 'T-', 'T-4a', 'A1-2', '-1'])('rejects the id %j', (id) => {
    expect(taskSchema.safeParse({ ...minimalTask, id }).success).toBe(false)
  })

  it('accepts an id of 32 characters and rejects a longer one', () => {
    const id = `${'A'.repeat(29)}-12`
    expect(taskSchema.parse({ ...minimalTask, id }).id).toBe(id)
    const result = taskSchema.safeParse({ ...minimalTask, id: `A${id}` })
    expect(result.success).toBe(false)
  })

  it('names both id forms when an id is not valid', () => {
    const result = taskSchema.safeParse({ ...minimalTask, id: 'T42' })
    expect(result.error?.issues[0]?.message).toBe('Task id looks like T-42 or ABC-42')
  })

  it.each(['T-1', 'T-0001', 'ABC-1', 'TASK-0005'])('accepts the id %j', (id) => {
    expect(taskSchema.parse({ ...minimalTask, id }).id).toBe(id)
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

  it('takes a title of 1 to 400 characters', () => {
    const parse = (title: string) => taskSchema.safeParse({ ...minimalTask, title })
    expect(parse('a'.repeat(400)).success).toBe(true)
    for (const title of ['', '   ', 'a'.repeat(401)]) {
      expect(parse(title).success).toBe(false)
    }
  })

  it.each(['https://github.com/o/r/pull/1', 'http://git.example.com/o/r/pull/1'])(
    'keeps a pr at %s',
    (pr) => {
      expect(taskSchema.parse({ ...minimalTask, pr }).pr).toBe(pr)
    },
  )

  // A click on the link would open the handler of its scheme on the user's machine. A bad
  // stored value drops the link, so the task list still loads.
  it.each(['vscode://file/etc/passwd', 'file:///etc/passwd', 'javascript:alert(1)', 'pull 12'])(
    'drops a pr at %s',
    (pr) => {
      expect(taskSchema.parse({ ...minimalTask, pr }).pr).toBeNull()
    },
  )

  it('drops an issue link with a URL that is not http or https', () => {
    const issue = { provider: 'jira', key: 'ABC-1', url: 'vscode://file/etc/passwd' }
    expect(taskSchema.parse({ ...minimalTask, issue }).issue).toBeNull()
  })
})

describe('issueLinkSchema', () => {
  const link = (url: string) => issueLinkSchema.safeParse({ provider: 'jira', key: 'ABC-1', url })

  it.each(['https://jira.example.com/browse/ABC-1', 'http://10.0.0.5:8080/browse/ABC-1'])(
    'takes %s',
    (url) => {
      expect(link(url).success).toBe(true)
    },
  )

  it.each(['vscode://file/etc/passwd', 'file:///etc/passwd', 'javascript:alert(1)'])(
    'refuses %s',
    (url) => {
      expect(link(url).success).toBe(false)
    },
  )
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

describe('splitTaskId', () => {
  it.each([
    ['Task-0005: Fix bug xxx', 'TASK-0005', 'Fix bug xxx'],
    ['Task-0005 Fix bug xxx', 'TASK-0005', 'Fix bug xxx'],
    ['  abc-12 : Fix it ', 'ABC-12', 'Fix it'],
    ['abc-12 Fix it', 'ABC-12', 'Fix it'],
    ['Task-0005:Fix', 'TASK-0005', 'Fix'],
    ['aBc-12:Fix it', 'ABC-12', 'Fix it'],
    ['abc-12\tFix it', 'ABC-12', 'Fix it'],
    ['Task-5: a: b', 'TASK-5', 'a: b'],
    ['Task-5: fix T-12 parser', 'TASK-5', 'fix T-12 parser'],
    ['COVID-19 tracker', 'COVID-19', 'tracker'],
  ])('splits %j', (title, id, rest) => {
    expect(splitTaskId(title)).toEqual({ id, title: rest })
  })

  it.each([
    'Fix bug xxx',
    'Fix T-12 parser',
    'Task-0005',
    'Task-0005:',
    ' Task-0005 : ',
    'Task-0005:   ',
    'A-1x y',
    'T-12x parser',
    'abc-12, Fix it',
    'abc-12- Fix it',
    'abc-12.Fix it',
    'abc-12-3 x',
    'A1-2 x',
    'ABC12 x',
    'Tâsk-1: x',
  ])('leaves %j as a plain title', (title) => {
    expect(splitTaskId(title)).toEqual({ id: null, title: title.trim() })
  })
})
