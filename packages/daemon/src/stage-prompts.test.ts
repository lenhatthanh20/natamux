import { DEFAULT_PIPELINE, defaultPrompt, type Stage, stagesOf } from '@natamux/core'
import { describe, expect, it } from 'vitest'
import { contextPrompt, manualContext, stagePrompt } from './stage-prompts.js'

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

describe('stagePrompt of the catalog', () => {
  it('takes the prompt of a catalog stage that is not in the default pipeline', () => {
    const prompt = stagePrompt({ id: 'security', kind: 'agent', gate: 'manual' }, task)
    expect(prompt).toContain('.natamux-task/security.md')
  })

  it('keeps unknown placeholders, long text and other languages as they are', () => {
    const long = 'x'.repeat(12_000)
    const stage: Stage = {
      id: 'plan',
      kind: 'agent',
      gate: 'manual',
      prompt: `Kiểm thử {foo} 🚀 ${long}`,
    }
    expect(stagePrompt(stage, task)).toBe(
      `Kiểm thử {foo} 🚀 ${long}\nWhen you are done, run \`natamux done\`.`,
    )
  })

  it('still adds the done line to a prompt that asks for it already', () => {
    const stage: Stage = {
      id: 'plan',
      kind: 'agent',
      gate: 'manual',
      prompt: 'Plan it. Then run `natamux done`.',
    }
    expect(stagePrompt(stage, task).endsWith('\nWhen you are done, run `natamux done`.')).toBe(true)
  })

  it('gives the web app and the agent the same default prompt', () => {
    const plan: Stage = { id: 'plan', kind: 'agent', gate: 'manual', output: 'plan.md' }
    const expected = defaultPrompt(plan)
      .replace('{title}', task.title)
      .replaceAll('{dir}', '.natamux-task')
    expect(stagePrompt(plan, task)).toBe(`${expected}\nWhen you are done, run \`natamux done\`.`)
  })

  it('leaves skill placeholders for the agent to fill', () => {
    const stage: Stage = { id: 'plan', kind: 'agent', gate: 'manual', prompt: 'Use {skill:plan}' }
    expect(stagePrompt(stage, task)).toContain('Use {skill:plan}')
  })
})

describe('contextPrompt', () => {
  const source = { type: 'skill', name: 'Ticket', skill: 'fetch-ticket' } as const
  const withRef = { ...task, context: { source: 'skill' as const, ref: 'ABC-1' } }

  it('calls the skill with the reference', () => {
    expect(contextPrompt(source, withRef).startsWith('/fetch-ticket ABC-1\n')).toBe(true)
    expect(
      contextPrompt(source, withRef, [], { kind: 'call', text: '$fetch-ticket' }).split('\n')[0],
    ).toBe('$fetch-ticket ABC-1')
  })

  it('gives the skill file to an agent that cannot call it, and names a missing skill', () => {
    const file = { kind: 'file', path: '.claude/skills/fetch-ticket/SKILL.md' } as const
    expect(contextPrompt(source, withRef, [], file).split('\n')[0]).toBe(
      'Read .claude/skills/fetch-ticket/SKILL.md and follow it for ABC-1.',
    )
    expect(contextPrompt(source, withRef, [], { kind: 'missing' }).split('\n')[0]).toBe(
      'Use the fetch-ticket skill for ABC-1.',
    )
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
