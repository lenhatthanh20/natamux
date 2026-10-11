import type { Task } from '@natamux/core'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { cleanup, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import type { ReactNode } from 'react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import type { DetailsMode } from '../lib/layout-mode'
import type { DetailsView } from '../lib/view-store'
import { TaskDetail } from './TaskDetail'

const TASK: Task = {
  id: 'T-0001',
  title: 'Fix the login',
  description: '',
  state: 'review',
  priority: 'normal',
  labels: [],
  agent: null,
  branch: 'natamux/t-0001',
  workspace: '/work/T-0001',
  pr: null,
  prStatus: null,
  stage: 'review',
  stages: ['implement', 'review', 'release'],
  lanes: {},
  stageSince: null,
  backgroundSince: null,
  context: null,
  issue: null,
  parentId: null,
  repo: null,
  dependsOn: [],
  createdAt: '2026-10-10T08:00:00.000Z',
  updatedAt: '2026-10-10T08:00:00.000Z',
}

afterEach(() => {
  cleanup()
  globalThis.localStorage.clear()
})

type DetailsOptions = { view?: DetailsView; progress?: ReactNode; mode?: DetailsMode }

function details({ view, progress, mode }: DetailsOptions = {}) {
  const onViewChange = vi.fn()
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  render(
    <QueryClientProvider client={client}>
      <TaskDetail
        task={TASK}
        now={new Date('2026-10-10T09:00:00.000Z')}
        busy={false}
        loadEvents={async () => []}
        onAction={vi.fn()}
        onCreateWorkspace={vi.fn()}
        onRemoveWorkspace={vi.fn()}
        onViewChanges={vi.fn()}
        onStartAgent={vi.fn()}
        onEdit={vi.fn()}
        onCheckPullRequest={vi.fn()}
        onClose={vi.fn()}
        mode={mode}
        view={view}
        onViewChange={onViewChange}
        progress={progress}
      />
    </QueryClientProvider>,
  )
  return { onViewChange }
}

describe('TaskDetail views', () => {
  it('shows the progress view, with a tab to go to the details', async () => {
    const { onViewChange } = details({ view: 'progress', progress: <p>Stage list</p> })
    const user = userEvent.setup()

    expect(screen.getByRole('tab', { name: 'Progress' }).getAttribute('aria-selected')).toBe('true')
    expect(screen.getByText('Stage list')).toBeTruthy()
    expect(screen.queryByText('No notes. Add some with Edit.')).toBeNull()

    await user.click(screen.getByRole('tab', { name: 'Details' }))
    expect(onViewChange).toHaveBeenCalledWith('details')
  })

  it('shows the details view without the progress', () => {
    details({ view: 'details', progress: <p>Stage list</p> })

    expect(screen.getByRole('tab', { name: 'Details' }).getAttribute('aria-selected')).toBe('true')
    expect(screen.getByText('No notes. Add some with Edit.')).toBeTruthy()
    expect(screen.queryByText('Stage list')).toBeNull()
  })

  it('shows the edit form over the progress view', async () => {
    details({ view: 'progress', progress: <p>Stage list</p> })
    const user = userEvent.setup()

    await user.click(screen.getByRole('button', { name: 'Edit' }))
    expect(screen.getByRole('form', { name: 'Edit T-0001' })).toBeTruthy()
    expect(screen.getByRole('tab', { name: 'Details' }).getAttribute('aria-selected')).toBe('true')
    expect(screen.queryByText('Stage list')).toBeNull()
  })

  it('keeps the edit form and its changes when the Progress tab is clicked', async () => {
    const { onViewChange } = details({ view: 'progress', progress: <p>Stage list</p> })
    const user = userEvent.setup()

    await user.click(screen.getByRole('button', { name: 'Edit' }))
    await user.type(screen.getByLabelText('Notes (markdown)'), 'New notes')
    await user.click(screen.getByRole('tab', { name: 'Progress' }))

    expect(onViewChange).not.toHaveBeenCalled()
    expect(screen.getByRole('form', { name: 'Edit T-0001' })).toBeTruthy()
    expect((screen.getByLabelText('Notes (markdown)') as HTMLTextAreaElement).value).toBe(
      'New notes',
    )
  })

  it('moves between the views with the arrow keys, from one tab stop', async () => {
    const { onViewChange } = details({ view: 'progress', progress: <p>Stage list</p> })
    const user = userEvent.setup()

    const progressTab = screen.getByRole('tab', { name: 'Progress' })
    expect(progressTab.getAttribute('tabindex')).toBe('0')
    expect(screen.getByRole('tab', { name: 'Details' }).getAttribute('tabindex')).toBe('-1')
    progressTab.focus()
    await user.keyboard('{ArrowRight}')
    expect(onViewChange).toHaveBeenCalledWith('details')
  })

  it.each(['docked', 'sheet', 'page'] as const)('shows the progress in %s mode', (mode) => {
    details({ view: 'progress', progress: <p>Stage list</p>, mode })

    expect(screen.getByRole('tab', { name: 'Progress' }).getAttribute('aria-selected')).toBe('true')
    expect(screen.getByText('Stage list')).toBeTruthy()
  })

  it('has no view tabs when there is no progress view', () => {
    details()

    expect(screen.queryByRole('tablist')).toBeNull()
    expect(screen.getByText('No notes. Add some with Edit.')).toBeTruthy()
  })
})
