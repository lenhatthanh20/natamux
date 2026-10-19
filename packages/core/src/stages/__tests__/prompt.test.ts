import { describe, expect, it } from 'vitest'
import { defaultPrompt } from '../prompt.js'

describe('defaultPrompt', () => {
  it('asks the spec for a code map', () => {
    expect(defaultPrompt({ id: 'spec', kind: 'agent' })).toContain('Code map')
  })

  it('takes the prompt of the catalog', () => {
    expect(defaultPrompt({ id: 'plan', kind: 'agent' })).toContain('Write {dir}/plan.md')
    expect(defaultPrompt({ id: 'security', kind: 'agent' })).toContain('{dir}/security.md')
  })

  it('makes one for a stage of its own', () => {
    expect(defaultPrompt({ id: 'sign-off', kind: 'agent', output: 'sign-off.md' })).toBe(
      'Task: {title}\nDo the Sign off stage of this task.\nWrite {dir}/sign-off.md.',
    )
  })

  it('gives an older stage with a catalog id its own prompt when it does other work', () => {
    expect(defaultPrompt({ id: 'docs', kind: 'agent', output: 'docs.md' })).toBe(
      'Task: {title}\nDo the Documentation stage of this task.\nWrite {dir}/docs.md.',
    )
  })
})
