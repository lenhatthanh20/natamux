import type { Task } from '@natamux/core'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { cleanup, render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import type { ComponentProps, ReactNode } from 'react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { DetailsMode } from '../../../lib/layout-mode'
import type { DetailsView } from '../../../lib/view-store/helpers'
import { labelText, TaskDetail } from '../TaskDetail'

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
  paths: {},
  stageSince: null,
  backgroundSince: null,
  workflowStages: [],
  context: null,
  issue: null,
  parentId: null,
  repo: null,
  dependsOn: [],
  createdAt: '2026-10-10T08:00:00.000Z',
  updatedAt: '2026-10-10T08:00:00.000Z',
}

// Relative times read the clock, so the tests run at a set time.
beforeEach(() => {
  vi.setSystemTime(new Date('2026-10-10T09:00:00.000Z'))
})

afterEach(() => {
  cleanup()
  globalThis.localStorage.clear()
  vi.useRealTimers()
})

type DetailsOptions = {
  view?: DetailsView
  progress?: ReactNode
  mode?: DetailsMode
  task?: Task
  more?: Partial<ComponentProps<typeof TaskDetail>>
}

function details({ view, progress, mode, task = TASK, more }: DetailsOptions = {}) {
  const onViewChange = vi.fn()
  const onDelete = vi.fn()
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  render(
    <QueryClientProvider client={client}>
      <TaskDetail
        task={task}
        busy={false}
        loadEvents={async () => []}
        onAction={vi.fn()}
        onCreateWorkspace={vi.fn()}
        onRemoveWorkspace={vi.fn()}
        onViewChanges={vi.fn()}
        onStartAgent={vi.fn()}
        onEdit={vi.fn()}
        onCheckPullRequest={vi.fn()}
        onDelete={onDelete}
        onClose={vi.fn()}
        mode={mode}
        view={view}
        onViewChange={onViewChange}
        progress={progress}
        {...more}
      />
    </QueryClientProvider>,
  )
  return { onViewChange, onDelete }
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

  it('asks before it deletes the task', async () => {
    const { onDelete } = details()
    const user = userEvent.setup()

    await user.click(screen.getByRole('button', { name: 'More actions for T-0001' }))
    await user.click(screen.getByRole('menuitem', { name: 'Delete task' }))
    expect(onDelete).not.toHaveBeenCalled()

    const dialog = screen.getByRole('dialog', { name: 'Delete T-0001?' })
    expect(within(dialog).getByText(/worktree folder/)).toBeTruthy()
    await user.click(within(dialog).getByRole('button', { name: 'Delete task' }))
    expect(onDelete).toHaveBeenCalledTimes(1)
  })

  it('has no view tabs when there is no progress view', () => {
    details()

    expect(screen.queryByRole('tablist')).toBeNull()
    expect(screen.getByText('No notes. Add some with Edit.')).toBeTruthy()
  })
})

describe('TaskDetail of a subtask and of a parent', () => {
  const branches = {
    branches: ['main', 'natamux/t-0001', 'other'],
    current: 'other',
    defaultBranch: 'main',
  }
  const subtask: Task = {
    ...TASK,
    id: 'T-0002',
    state: 'ready',
    workspace: null,
    parentId: 'T-0001',
  }
  const startField = () => screen.getByRole('combobox', { name: 'Start from branch' })

  it('starts from the branch of the parent when it is in the list', () => {
    details({ task: subtask, more: { branches, parentBranch: 'natamux/t-0001' } })

    expect((startField() as HTMLInputElement).value).toBe('natamux/t-0001 (default)')
  })

  it('starts from the default branch when the parent branch is not in the list', () => {
    details({ task: subtask, more: { branches, parentBranch: 'natamux/gone' } })

    expect((startField() as HTMLInputElement).value).toBe('main (default)')
  })

  it('says a ready parent waits for its open subtask', () => {
    details({ task: { ...TASK, state: 'ready' }, more: { waitingFor: ['T-0002'] } })

    expect(screen.getByText('Waits for T-0002 to be done')).toBeTruthy()
  })
})

describe('labelText', () => {
  it.each([
    ['tries-t-0002', 'Tries of T-0002'],
    ['tries-task-0005', 'Tries of TASK-0005'],
    ['tries-a1-2', 'tries-a1-2'],
    ['tries-task', 'tries-task'],
    ['perf', 'perf'],
  ])('shows %s as %s', (label, text) => {
    expect(labelText(label)).toBe(text)
  })
})
