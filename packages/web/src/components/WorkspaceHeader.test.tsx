import type { Task } from '@natamux/core'
import { cleanup, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { WorkspaceHeader } from './WorkspaceHeader'

afterEach(cleanup)

const TASK: Task = {
  id: 'T-0002',
  title: 'Add dark mode to settings',
  description: '',
  state: 'running',
  priority: 'normal',
  labels: [],
  agent: 'claude-code',
  branch: 'natamux/t-0002-add-dark-mode',
  workspace: '/work/T-0002',
  pr: null,
  prStatus: null,
  stage: 'implement',
  stages: ['implement'],
  lanes: {},
  stageSince: null,
  backgroundSince: null,
  context: null,
  issue: null,
  parentId: null,
  repo: null,
  dependsOn: [],
  createdAt: '2026-10-10T08:00:00.000Z',
  updatedAt: '2026-10-10T08:20:00.000Z',
}

function header({ ahead = 0, uncommitted = 0 }: { ahead?: number; uncommitted?: number }) {
  const handlers = { onPush: vi.fn(), onPullRequest: vi.fn(), onCommit: vi.fn() }
  render(
    <WorkspaceHeader
      task={TASK}
      ahead={ahead}
      base="main"
      uncommitted={uncommitted}
      pending={null}
      {...handlers}
    />,
  )
  return handlers
}

const pullRequest = () => screen.getByRole('button', { name: 'Create pull request' })

describe('WorkspaceHeader', () => {
  it('puts Commit in place of Push while changes are not committed and nothing is ahead', async () => {
    const { onCommit, onPush } = header({ uncommitted: 2 })

    expect(screen.queryByRole('button', { name: /^Push/ })).toBeNull()
    await userEvent.setup().click(screen.getByRole('button', { name: 'Commit…' }))

    expect(onCommit).toHaveBeenCalledTimes(1)
    expect(onPush).not.toHaveBeenCalled()
    expect(pullRequest().getAttribute('aria-disabled')).toBe('true')
    expect(pullRequest().dataset.tip).toBe('Commit your 2 changed files first')
  })

  it('says there is nothing to push when the workspace matches its last commit', () => {
    header({})

    const push = screen.getByRole('button', { name: 'Push' })
    expect(push.dataset.tip).toBe('Nothing to push: 0 commits ahead of main')
    expect(pullRequest().dataset.tip).toBe('Push first: 0 commits ahead of main')
    expect(screen.queryByRole('button', { name: 'Commit…' })).toBeNull()
  })

  it('pushes the commits that are ahead, with or without other changes', async () => {
    const { onPush } = header({ ahead: 2, uncommitted: 1 })

    await userEvent.setup().click(screen.getByRole('button', { name: 'Push ↑2' }))

    expect(onPush).toHaveBeenCalledTimes(1)
    expect(pullRequest().getAttribute('aria-disabled')).toBeNull()
  })

  it('draws the pull request button as primary only when it can be used', () => {
    header({})
    expect(pullRequest().dataset.variant).toBe('secondary')
    cleanup()

    header({ ahead: 1 })
    expect(pullRequest().dataset.variant).toBe('primary')
  })
})
