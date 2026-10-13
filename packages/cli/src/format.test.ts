import { taskSchema } from '@natamux/core'
import { describe, expect, it } from 'vitest'
import { formatTask } from './format.js'

const task = taskSchema.parse({
  id: 'T-0007',
  title: 'Export tasks',
  createdAt: '2026-10-09T09:00:00.000Z',
  updatedAt: '2026-10-09T09:00:00.000Z',
  stage: 'spec',
  stages: ['context', 'spec'],
  context: { source: 'github-issue', ref: '#42' },
  issue: { provider: 'github', key: '#42', url: 'https://github.com/o/r/issues/42' },
})

describe('formatTask', () => {
  it('shows the stage, the issue, the context and the repository', () => {
    const text = formatTask(task)
    expect(text).toContain('Stage:      spec')
    expect(text).toContain('Issue:      #42 https://github.com/o/r/issues/42')
    expect(text).toContain('Context:    github-issue #42')
    expect(text).toContain('Repo:       default')
  })

  it('shows a dash for a missing issue and context', () => {
    const text = formatTask({ ...task, issue: null, context: null })
    expect(text).toContain('Issue:      -')
    expect(text).toContain('Context:    -')
  })

  it('names the skill of the context, picked or from the settings', () => {
    const picked = { source: 'skill' as const, skill: 'jira-ticket', ref: 'ABC-1' }
    expect(formatTask({ ...task, context: picked })).toContain(
      'Context:    jira-ticket (skill) ABC-1',
    )
    const configured = { source: 'skill' as const, name: 'Linear', ref: 'LIN-7' }
    expect(formatTask({ ...task, context: configured })).toContain(
      'Context:    Linear (skill) LIN-7',
    )
  })
})
