import { describe, expect, it } from 'vitest'
import { commentForAgent } from './AgentComment'

describe('commentForAgent', () => {
  it('names the line when there is one', () => {
    expect(commentForAgent('src/a.ts', 'Too long.', 12)).toBe(
      'Review comment on src/a.ts, line 12:\nToo long.',
    )
  })

  it('names the file, then the comment', () => {
    expect(commentForAgent('src/a.ts', '  Rename x.\n')).toBe(
      'Review comment on src/a.ts:\nRename x.',
    )
  })
})
