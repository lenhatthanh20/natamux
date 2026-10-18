import { mkdtempSync, readFileSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { EventBus } from '../event-bus.js'
import { TaskIndex } from '../task-index.js'
import { TaskService } from '../task-service.js'
import { TemplateNotFoundError, TemplateService } from '../template-service.js'

let dataDir: string
let index: TaskIndex
let tasks: TaskService
let templates: TemplateService

beforeEach(() => {
  dataDir = mkdtempSync(join(tmpdir(), 'natamux-templates-'))
  index = new TaskIndex(':memory:')
  tasks = new TaskService({ dataDir, index, bus: new EventBus() })
  templates = new TemplateService({ dataDir, tasks })
})

afterEach(() => {
  index.close()
  rmSync(dataDir, { recursive: true, force: true })
})

describe('TemplateService', () => {
  it('has no templates at first', () => {
    expect(templates.list()).toEqual([])
  })

  it('saves a template as a markdown file and reads it back', () => {
    const saved = templates.save({
      name: 'Bug fix',
      title: 'Fix',
      description: 'Find the cause first.',
      priority: 'high',
      labels: ['bug'],
    })

    expect(saved).toEqual({
      id: 'bug-fix',
      name: 'Bug fix',
      title: 'Fix',
      description: 'Find the cause first.',
      priority: 'high',
      labels: ['bug'],
    })
    expect(readFileSync(join(dataDir, 'templates', 'bug-fix.md'), 'utf8')).toContain(
      'Find the cause first.',
    )
    expect(templates.list()).toEqual([saved])
  })

  it('replaces a template with the same name, and sorts by name', () => {
    templates.save({ name: 'Zeta', title: 'Z' })
    templates.save({ name: 'Alpha', title: 'A' })
    templates.save({ name: 'Alpha', title: 'A2' })

    expect(templates.list().map((template) => [template.name, template.title])).toEqual([
      ['Alpha', 'A2'],
      ['Zeta', 'Z'],
    ])
  })

  it('makes a draft task from a template, with a new title', () => {
    templates.save({ name: 'Bug fix', title: 'Fix', description: 'Steps', labels: ['bug'] })

    const task = templates.createTask('bug-fix', { title: 'Fix the login' })

    expect(task).toMatchObject({
      title: 'Fix the login',
      description: 'Steps',
      labels: ['bug'],
      priority: 'normal',
      state: 'draft',
    })
    expect(task.repo).toBeNull()
  })

  it('takes the id from the start of a title, and gives a plain title the next id', () => {
    templates.save({ name: 'Bug fix', title: 'Fix' })

    expect(templates.createTask('bug-fix', { title: 'abc-3: Fix it' })).toMatchObject({
      id: 'ABC-3',
      title: 'Fix it',
    })
    expect(templates.createTask('bug-fix', { title: 'Fix more' }).id).toBe('T-0001')
  })

  it('puts a task from a template in the repository it is given', () => {
    templates.save({ name: 'Bug fix', title: 'Fix' })

    expect(templates.createTask('bug-fix', { title: 'Fix it', repo: 'web' }).repo).toBe('web')
  })

  it('refuses an unknown template and an empty title', () => {
    expect(() => templates.createTask('nope', { title: 'x' })).toThrow(TemplateNotFoundError)
    templates.save({ name: 'Bug fix', title: 'Fix' })
    expect(() => templates.createTask('bug-fix', { title: ' ' })).toThrow()
  })
})
