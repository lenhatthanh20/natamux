import { cleanup, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'
import type { CodeSelection } from '../../../lib/changes'
import { AgentComment, commentForAgent, QUOTE_MAX } from '../AgentComment'

afterEach(cleanup)

const quote = (over: Partial<CodeSelection> = {}): CodeSelection => ({
  path: 'src/a.ts',
  side: 'new',
  start: 12,
  end: 14,
  text: 'const a = 1\nconst b = 2\nconst c = 3',
  ...over,
})

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

  it('quotes the selected lines between the heading and the comment', () => {
    expect(commentForAgent('src/a.ts', 'Why three?', 3, quote())).toBe(
      'Review comment on src/a.ts, lines 12-14:\n```\nconst a = 1\nconst b = 2\nconst c = 3\n```\nWhy three?',
    )
  })

  it('names one line, and the old side', () => {
    expect(
      commentForAgent(
        'src/a.ts',
        'Why?',
        null,
        quote({ side: 'old', start: 5, end: 5, text: 'x' }),
      ),
    ).toBe('Review comment on src/a.ts, line 5 before the change:\n```\nx\n```\nWhy?')
  })

  it('uses a longer fence than any backticks in the quote', () => {
    const text = 'a\n```js\nb\n````\nc'
    expect(commentForAgent('a.md', 'Ok?', null, quote({ text }))).toBe(
      `Review comment on a.md, lines 12-14:\n\`\`\`\`\`\n${text}\n\`\`\`\`\`\nOk?`,
    )
  })

  it('sends only the line numbers for a quote that is too long', () => {
    const text = 'x'.repeat(QUOTE_MAX + 1)
    expect(commentForAgent('src/a.ts', 'Split it.', null, quote({ text }))).toBe(
      'Review comment on src/a.ts, lines 12-14:\n(Too long to quote. Read these lines in the file.)\nSplit it.',
    )
    expect(commentForAgent('src/a.ts', 'Hm.', null, quote({ text, side: 'old' }))).toBe(
      'Review comment on src/a.ts, lines 12-14 before the change:\n(Too long to quote. Read these lines in the last commit.)\nHm.',
    )
  })
})

describe('AgentComment', () => {
  it('names the lines of the quote, and drops it on remove', async () => {
    const onClearQuote = vi.fn()
    const user = userEvent.setup()
    render(
      <AgentComment
        path="src/a.ts"
        line={3}
        quote={quote({ side: 'old' })}
        onClearQuote={onClearQuote}
        send={async () => undefined}
      />,
    )
    expect(
      screen.getByLabelText('Comment on lines 12–14 (before the change) for the agent'),
    ).toBeTruthy()
    expect(screen.getByText('Lines 12–14 (before the change)')).toBeTruthy()
    await user.click(screen.getByRole('button', { name: 'Remove the selected lines' }))
    expect(onClearQuote).toHaveBeenCalledOnce()
  })

  it('says when the quote is too long to send', () => {
    render(
      <AgentComment
        path="src/a.ts"
        quote={quote({ text: 'x'.repeat(QUOTE_MAX + 1) })}
        onClearQuote={() => {}}
        send={async () => undefined}
      />,
    )
    expect(screen.getByText(/too long to quote, only the line numbers are sent/)).toBeTruthy()
  })

  it('keeps the quote when the send fails, and clears it when it works', async () => {
    const onClearQuote = vi.fn()
    const send = vi.fn().mockRejectedValueOnce(new Error('No agent runs')).mockResolvedValue({})
    const user = userEvent.setup()
    render(
      <AgentComment
        path="src/a.ts"
        quote={quote({ start: 7, end: 7 })}
        onClearQuote={onClearQuote}
        send={send}
      />,
    )
    const field = screen.getByLabelText('Comment on line 7 for the agent')
    await user.type(field, 'Why?')
    await user.click(screen.getByRole('button', { name: /Send to agent/ }))
    expect(await screen.findByText('No agent runs')).toBeTruthy()
    expect(onClearQuote).not.toHaveBeenCalled()
    expect(send).toHaveBeenLastCalledWith(
      'Review comment on src/a.ts, line 7:\n```\nconst a = 1\nconst b = 2\nconst c = 3\n```\nWhy?',
    )

    await user.click(screen.getByRole('button', { name: /Send to agent/ }))
    expect(await screen.findByText('Sent to the agent.')).toBeTruthy()
    expect(onClearQuote).toHaveBeenCalledOnce()
  })
})
