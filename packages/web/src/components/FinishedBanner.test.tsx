import type { Task } from '@natamux/core'
import { cleanup, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import type { ComponentProps } from 'react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import type { GitFacts } from '../lib/finished'
import { FinishedBanner } from './FinishedBanner'

afterEach(cleanup)

const PR = 'https://github.com/o/r/pull/310'
const task: Pick<Task, 'state' | 'pr' | 'prStatus' | 'branch'> = {
  state: 'done',
  pr: PR,
  prStatus: { state: 'merged', checks: 'passing' },
  branch: 'natamux/t-0058-icons',
}
const clean: GitFacts = {
  uncommitted: 0,
  graph: { ahead: 0, base: 'main', push: { remote: 'tracked', unpushed: 0, unpulled: 0 } },
}

function banner(props: Partial<ComponentProps<typeof FinishedBanner>> = {}) {
  const handlers = {
    onCommit: vi.fn(),
    onPush: vi.fn(),
    onCreatePullRequest: vi.fn(),
    onRemove: vi.fn(),
  }
  render(
    <FinishedBanner
      task={task}
      facts={clean}
      pushLabel="Push ↑2"
      pending={null}
      {...handlers}
      {...props}
    />,
  )
  return handlers
}

describe('FinishedBanner', () => {
  it('says the task is done in the done color, and offers to remove the workspace', async () => {
    const { onRemove } = banner()

    const status = screen.getByRole('status')
    expect(status.textContent).toContain('Task done.')
    expect(status.textContent).toContain('Pull request #310 is merged.')
    expect(status.dataset.state).toBe('done')
    await userEvent.setup().click(screen.getByRole('button', { name: 'Remove workspace' }))
    expect(onRemove).toHaveBeenCalledTimes(1)
  })

  it('has the cancelled color for a cancelled task', () => {
    banner({ task: { ...task, state: 'cancelled' } })

    expect(screen.getByRole('status').dataset.state).toBe('cancelled')
    expect(screen.getByRole('status').textContent).toContain('Task cancelled.')
  })

  it('asks for a commit first, and says why Remove workspace waits', async () => {
    const { onCommit, onRemove } = banner({ facts: { ...clean, uncommitted: 3 } })
    const user = userEvent.setup()

    await user.click(screen.getByRole('button', { name: 'Commit…' }))
    expect(onCommit).toHaveBeenCalledTimes(1)
    const remove = screen.getByRole('button', { name: 'Remove workspace' })
    expect(remove.getAttribute('aria-disabled')).toBe('true')
    expect(screen.getByText('Commit or discard them first')).toBeTruthy()
    await user.click(remove)
    expect(onRemove).not.toHaveBeenCalled()
  })

  it('pushes with the label of the header', async () => {
    const facts: GitFacts = {
      uncommitted: 0,
      graph: { ahead: 2, base: 'main', push: { remote: 'tracked', unpushed: 2, unpulled: 0 } },
    }
    const { onPush } = banner({ task: { ...task, pr: null, prStatus: null }, facts })

    await userEvent.setup().click(screen.getByRole('button', { name: 'Push ↑2' }))
    expect(onPush).toHaveBeenCalledTimes(1)
    expect(screen.getByRole('button', { name: 'Remove workspace' })).toBeTruthy()
  })

  it('links to a pull request that is still open', () => {
    banner({ task: { ...task, prStatus: { state: 'open', checks: 'passing' } } })

    const link = screen.getByRole('link', { name: 'Open pull request' })
    expect(link.getAttribute('href')).toBe(PR)
    expect(link.getAttribute('target')).toBe('_blank')
  })

  it('offers only Remove workspace while git is read', () => {
    banner({ facts: { uncommitted: null, graph: null } })

    expect(screen.getByRole('status').textContent).not.toContain('Nothing is left')
    const buttons = screen.getAllByRole('button').map((button) => button.textContent)
    expect(buttons).toEqual(['Remove workspace'])
    // The next step is not known yet, so nothing is the main button.
    expect(screen.getByRole('button', { name: 'Remove workspace' }).dataset.variant).toBe(
      'secondary',
    )
  })
})
