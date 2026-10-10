import { DEFAULT_PIPELINE, type Stage, stagesOf } from '@natamux/core'
import { describe, expect, it } from 'vitest'
import { manualContext, stagePrompt } from './stage-prompts.js'

const task = { title: 'Export tasks', description: '', context: null }

function defaultStage(id: string): Stage {
  const stage = DEFAULT_PIPELINE.flatMap(stagesOf).find((each) => each.id === id)
  return stage ?? { id, kind: 'agent', gate: 'auto' }
}

describe('stagePrompt', () => {
  it('tells the implement agent not to commit by default', () => {
    const prompt = stagePrompt(defaultStage('implement'), task)
    expect(prompt).toContain('Do not commit')
    expect(prompt).not.toContain('Commit your work')
    expect(prompt.endsWith('When you are done, run `natamux done`.')).toBe(true)
  })

  it('tells the testing agent not to commit by default', () => {
    const prompt = stagePrompt(defaultStage('testing'), task)
    expect(prompt).toContain('Do not commit')
    expect(prompt).not.toContain('Commit your work')
  })

  it('asks the agent to commit when the stage needs a commit', () => {
    const prompt = stagePrompt({ ...defaultStage('implement'), requireCommit: true }, task)
    expect(prompt).toContain('Commit your work when it is done.')
    expect(prompt).not.toContain('Do not commit')
  })

  it('keeps a prompt of your own as it is', () => {
    const stage: Stage = { ...defaultStage('implement'), prompt: 'Fix {title}. {commit}' }
    expect(stagePrompt(stage, task)).toBe(
      'Fix Export tasks. {commit}\nWhen you are done, run `natamux done`.',
    )
  })

  it('says nothing about commits in the spec prompt', () => {
    const prompt = stagePrompt(defaultStage('spec'), task)
    expect(prompt).toContain('Do not change any code.')
    expect(prompt.toLowerCase()).not.toContain('commit')
  })
})

describe('manualContext', () => {
  it('writes the title and the notes, keeping their line breaks', () => {
    const text = manualContext({
      title: 'Fix the login',
      description: '  Keep the old URL.\nAsk before a redirect.  ',
      context: null,
    })
    expect(text).toBe('# Fix the login\n\nKeep the old URL.\nAsk before a redirect.\n')
  })

  it('says there are no notes when the notes are empty or only spaces', () => {
    for (const description of ['', '  \n\t\n']) {
      expect(manualContext({ title: 'Fix the login', description, context: null })).toBe(
        '# Fix the login\n\n(No notes.)\n',
      )
    }
  })
})
