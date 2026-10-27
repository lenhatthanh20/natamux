import type { ReviewComment } from '@natamux/core'
import { cleanup, render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { ReviewComments, type ReviewCommentsProps } from '../ReviewComments'

afterEach(cleanup)

const make = (over: Partial<ReviewComment>): ReviewComment => ({
  id: 'c1',
  path: 'src/a.ts',
  side: 'new',
  start: 3,
  end: 5,
  quote: 'one\ntwo',
  body: 'Rename this',
  state: 'draft',
  createdAt: '2026-10-27T09:00:00Z',
  ...over,
})

function setup(over: Partial<ReviewCommentsProps> = {}) {
  const props: ReviewCommentsProps = {
    comments: [
      make({}),
      make({ id: 'c2', path: 'src/b.ts', start: 9, end: 9, state: 'sent', outdated: true }),
      make({ id: 'c3', path: 'src/b.ts', start: 1, end: 1, state: 'resolved' }),
    ],
    showResolved: false,
    onShowResolved: vi.fn(),
    editingId: null,
    onEditing: vi.fn(),
    onOpen: vi.fn(),
    onSave: vi.fn().mockResolvedValue(true),
    onDelete: vi.fn(),
    onResolve: vi.fn(),
    onReopen: vi.fn(),
    onSend: vi.fn(),
    sending: false,
    error: null,
    ...over,
  }
  render(<ReviewComments {...props} />)
  return props
}

describe('ReviewComments', () => {
  it('shows the open count, groups by file and hides resolved ones', () => {
    setup()
    const section = screen.getByRole('region', { name: 'Comments' })
    expect(within(section).getByText('2 open')).toBeTruthy()
    expect(within(section).getByRole('list', { name: 'Comments on src/a.ts' })).toBeTruthy()
    expect(within(section).queryByText(/^c3/)).toBeNull()
    expect(within(section).queryByRole('button', { name: 'Reopen c3' })).toBeNull()
  })

  it('shows resolved comments with Reopen when asked', async () => {
    const props = setup({ showResolved: true })
    await userEvent.click(screen.getByRole('button', { name: 'Reopen c3' }))
    expect(props.onReopen).toHaveBeenCalledWith('c3')
    await userEvent.click(screen.getByRole('checkbox', { name: 'Show resolved' }))
    expect(props.onShowResolved).toHaveBeenCalledWith(false)
  })

  it('has the buttons of each state, and the quote of an outdated comment', async () => {
    const props = setup()
    expect(screen.getByText(/outdated/)).toBeTruthy()
    expect(screen.getByText(/one\s+two/)).toBeTruthy()
    await userEvent.click(screen.getByRole('button', { name: 'Edit c1' }))
    expect(props.onEditing).toHaveBeenCalledWith('c1')
    await userEvent.click(screen.getByRole('button', { name: 'Resolve c2' }))
    expect(props.onResolve).toHaveBeenCalledWith('c2')
    await userEvent.click(screen.getByRole('button', { name: 'Delete c2' }))
    expect(props.onDelete).toHaveBeenCalledWith('c2')
  })

  it('opens the file of a comment', async () => {
    const props = setup()
    await userEvent.click(screen.getByRole('button', { name: /^c1 lines 3–5/ }))
    expect(props.onOpen).toHaveBeenCalledWith(expect.objectContaining({ id: 'c1' }))
  })

  it('saves an edited draft', async () => {
    const props = setup({ editingId: 'c1' })
    const box = screen.getByLabelText('Edit c1')
    await userEvent.clear(box)
    await userEvent.type(box, 'New text')
    await userEvent.click(screen.getByRole('button', { name: 'Save c1' }))
    expect(props.onSave).toHaveBeenCalledWith('c1', 'New text')
  })

  it('sends the drafts, and says why it cannot with none', async () => {
    const props = setup()
    await userEvent.click(screen.getByRole('button', { name: 'Send 1 comment' }))
    expect(props.onSend).toHaveBeenCalledOnce()
    cleanup()
    const none = setup({ comments: [make({ state: 'sent' })], error: 'No agent runs' })
    const button = screen.getByRole('button', { name: 'Send 0 comments' })
    expect(button.getAttribute('aria-disabled')).toBe('true')
    await userEvent.click(button)
    expect(none.onSend).not.toHaveBeenCalled()
    expect(screen.getByRole('alert').textContent).toBe('No agent runs')
  })
})
