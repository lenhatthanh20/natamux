import { describe, expect, it } from 'vitest'
import { manualContext } from './stage-prompts.js'

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
