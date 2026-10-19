import type { Task } from '@natamux/core'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { cleanup, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'
import type { StoredEvent } from '../../../lib/api'
import { NoWorkspace } from '../NoWorkspace'

afterEach(cleanup)

const task: Task = {
  id: 'T-0058',
  title: 'Improve the icons',
  description: '',
  state: 'done',
  priority: 'normal',
  labels: [],
  agent: 'claude-code',
  branch: null,
  workspace: null,
  pr: 'https://github.com/o/r/pull/310',
  prStatus: { state: 'merged', checks: 'passing' },
  stage: 'release',
  stages: ['implement', 'review', 'release'],
  lanes: {},
  paths: {},
  stageSince: null,
  backgroundSince: null,
  workflowStages: [],
  context: null,
  issue: null,
  parentId: null,
  repo: null,
  dependsOn: [],
  createdAt: '2026-10-14T09:00:00.000Z',
  updatedAt: '2026-10-14T09:00:00.000Z',
}

// Approved at noon UTC, so the day is the same in the time zone of any test machine.
const events: StoredEvent[] = [
  {
    seq: 1,
    accepted: true,
    event: {
      taskId: 'T-0058',
      source: 'human',
      type: 'stage.approved',
      at: '2026-10-14T12:00:00.000Z',
      payload: {},
    },
  },
]

function view(shown: Task = task) {
  const handlers = {
    onCreateWorkspace: vi.fn(),
    onStartAgent: vi.fn(),
    onShowProgress: vi.fn(),
    onFollowUp: vi.fn(),
  }
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  render(
    <QueryClientProvider client={client}>
      <NoWorkspace
        task={shown}
        busy={false}
        waitingFor={[]}
        loadEvents={async () => events}
        {...handlers}
      />
    </QueryClientProvider>,
  )
  return handlers
}

describe('NoWorkspace of a finished task', () => {
  it('sums up the task and offers where to go next', async () => {
    const { onShowProgress, onFollowUp } = view()
    const user = userEvent.setup()

    const heading = screen.getByRole('heading', { name: 'T-0058 is done' })
    expect(document.activeElement).toBe(heading)
    expect(screen.getByText('#310, merged, checks pass')).toBeTruthy()
    expect(
      screen.getByText('Its workspace was removed. The task, its notes, docs and history stay.'),
    ).toBeTruthy()
    expect(screen.getByText('Deleted')).toBeTruthy()
    expect(await screen.findByText(/^Oct 14, 2026/)).toBeTruthy()
    const link = screen.getByRole('link', { name: 'Open pull request' })
    expect(link.getAttribute('href')).toBe('https://github.com/o/r/pull/310')

    await user.click(screen.getByRole('button', { name: 'Show progress' }))
    expect(onShowProgress).toHaveBeenCalledTimes(1)
    await user.click(screen.getByRole('button', { name: 'Start a follow-up task' }))
    expect(onFollowUp).toHaveBeenCalledTimes(1)
    expect(screen.queryByRole('button', { name: 'Create workspace' })).toBeNull()
  })

  it('names a kept branch and a task with no pull request', () => {
    view({ ...task, state: 'cancelled', branch: 'natamux/t-0058-icons', pr: null, prStatus: null })

    expect(screen.getByRole('heading', { name: 'T-0058 was cancelled' })).toBeTruthy()
    expect(screen.getByText('None')).toBeTruthy()
    expect(screen.queryByRole('link', { name: 'Open pull request' })).toBeNull()
  })

  it('still offers a new workspace to a task that is not finished', () => {
    view({ ...task, state: 'ready', pr: null, prStatus: null })

    expect(screen.getByRole('button', { name: 'Create workspace' })).toBeTruthy()
    expect(screen.queryByRole('heading', { name: /is done/ })).toBeNull()
  })
})
