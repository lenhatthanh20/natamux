import { describe, expect, it } from 'vitest'
import { commentForAgent } from './AgentComment'

describe('commentForAgent', () => {
  it('names the file, then the comment', () => {
    expect(commentForAgent('src/a.ts', '  Rename x.\n')).toBe(
      'Review comment on src/a.ts:\nRename x.',
    )
  })
})
