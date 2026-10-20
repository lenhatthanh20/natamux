import type { Task } from '@natamux/core'
import { cleanup, fireEvent, render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ContextMenuHost } from '../../../components/ui/Menu'
import { rowLabel } from '../../../lib/tasks'
import { TaskTable } from '../TaskTable'

// Each row names itself once per render, so the calls count the renders of each row.
vi.mock('../../../lib/tasks', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../../../lib/tasks')>()
  return { ...actual, rowLabel: vi.fn(actual.rowLabel) }
})

afterEach(cleanup)

const NOW = new Date('2026-10-10T08:30:00.000Z')

// Relative times read the clock, so the tests run at a set time.
beforeEach(() => {
  vi.setSystemTime(NOW)
})

afterEach(() => {
  vi.useRealTimers()
})

function makeTask(overrides: Partial<Task> = {}): Task {
  return {
    id: 'T-0001',
    title: 'Fix flaky login test',
    description: '',
    state: 'failed',
    priority: 'normal',
    labels: [],
    agent: 'claude-code',
    branch: null,
    workspace: null,
    pr: null,
    prStatus: null,
    stage: 'spec',
    stages: ['spec'],
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
    updatedAt: '2026-10-10T08:20:00.000Z',
    ...overrides,
  }
}

const TASKS = [
  makeTask(),
  makeTask({ id: 'T-0002', title: 'Write release notes', state: 'ready', agent: null }),
]

function table(tasks: readonly Task[] = TASKS) {
  const handlers = {
    onSelect: vi.fn(),
    onOpen: vi.fn(),
    onToggleDetails: vi.fn(),
    onAddSubtask: vi.fn(),
    onAction: vi.fn(),
    onStartAgent: vi.fn(),
    onDelete: vi.fn(),
  }
  render(
    <>
      <TaskTable
        tasks={tasks}
        mainWidth={null}
        selectedId={null}
        sort={null}
        onSort={() => {}}
        {...handlers}
      />
      <ContextMenuHost />
    </>,
  )
  return handlers
}

const rowOf = (title: string) =>
  screen.getByRole('button', { name: new RegExp(title) }).closest('tr') as HTMLElement

describe('TaskTable', () => {
  it('shows the estimated cost of each task, with the tokens in its tip', () => {
    const usage = {
      'T-0001': { tokens: 2_400_000, costUsd: 1.84, unpricedModels: [], stages: [] },
      'T-0002': { tokens: 900, costUsd: null, unpricedModels: ['mystery'], stages: [] },
    }
    render(
      <TaskTable
        tasks={TASKS}
        mainWidth={null}
        usage={usage}
        selectedId={null}
        sort={null}
        onSort={() => {}}
        onSelect={() => {}}
        onOpen={() => {}}
        onToggleDetails={() => {}}
      />,
    )

    expect(screen.getByRole('columnheader', { name: 'Cost' })).toBeTruthy()
    const cost = within(rowOf('Fix flaky login test')).getByText('$1.84')
    expect(cost.dataset.tip).toBe('2.4M tokens, estimated at API list prices')
    expect(within(rowOf('Write release notes')).getByText('—')).toBeTruthy()
  })

  it('has no cost column when no task has usage', () => {
    table()
    expect(screen.queryByRole('columnheader', { name: 'Cost' })).toBeNull()
  })

  // The first click can open the details panel; the page then moves, and the second click of the
  // double click lands on the row next to it.
  it('opens the row of the first click of a double click', () => {
    const { onSelect, onOpen } = table()

    fireEvent.click(rowOf('Write release notes'), { detail: 1 })
    fireEvent.click(rowOf('Fix flaky login test'), { detail: 2 })
    fireEvent.doubleClick(rowOf('Fix flaky login test'))

    expect(onSelect.mock.calls.map(([id]) => id)).toEqual(['T-0002'])
    expect(onOpen).toHaveBeenCalledWith('T-0002')
  })

  it('opens the row of a double click', () => {
    const { onOpen } = table()

    fireEvent.click(rowOf('Fix flaky login test'), { detail: 1 })
    fireEvent.click(rowOf('Fix flaky login test'), { detail: 2 })
    fireEvent.doubleClick(rowOf('Fix flaky login test'))

    expect(onOpen).toHaveBeenCalledWith('T-0001')
  })

  it('names the agent as the pickers do, and a dash for no agent', () => {
    table()

    expect(within(rowOf('Fix flaky login test')).getByText('Claude Code')).toBeTruthy()
    expect(screen.queryByText('claude-code')).toBeNull()
    expect(within(rowOf('Write release notes')).getAllByText('-').length).toBeGreaterThan(0)
  })

  it('offers the task actions in the row menu, above the copy items', async () => {
    const { onAction, onStartAgent } = table()
    const user = userEvent.setup()

    await user.pointer({ keys: '[MouseRight]', target: rowOf('Fix flaky login test') })
    const labels = screen.getAllByRole('menuitem').map((item) => item.textContent ?? '')
    const retry = labels.findIndex((label) => label.startsWith('Retry'))
    expect(retry).toBeGreaterThan(-1)
    expect(labels.findIndex((label) => label.startsWith('Restart agent'))).toBeGreaterThan(retry)
    expect(labels.findIndex((label) => label.startsWith('Copy task id'))).toBeGreaterThan(retry)
    expect(labels.at(-2)).toBe('Cancel task')
    expect(labels.at(-1)).toBe('Delete task')

    await user.click(screen.getByRole('menuitem', { name: /^Retry/ }))
    expect(onAction).toHaveBeenCalledWith(
      expect.objectContaining({ id: 'T-0001' }),
      expect.objectContaining({ type: 'task.retried' }),
    )

    await user.pointer({ keys: '[MouseRight]', target: rowOf('Fix flaky login test') })
    await user.click(screen.getByRole('menuitem', { name: /^Restart agent/ }))
    expect(onStartAgent).toHaveBeenCalledWith(expect.objectContaining({ id: 'T-0001' }))
  })

  it('adds a subtask from the row menu of a task with no parent only', async () => {
    const { onAddSubtask } = table([
      makeTask(),
      makeTask({ id: 'T-0002', title: 'Write release notes', parentId: 'T-0001' }),
    ])
    const user = userEvent.setup()

    await user.pointer({ keys: '[MouseRight]', target: rowOf('Write release notes') })
    expect(screen.queryByRole('menuitem', { name: /^Add subtask/ })).toBeNull()
    await user.keyboard('{Escape}')

    await user.pointer({ keys: '[MouseRight]', target: rowOf('Fix flaky login test') })
    await user.click(screen.getByRole('menuitem', { name: /^Add subtask/ }))
    expect(onAddSubtask).toHaveBeenCalledWith(expect.objectContaining({ id: 'T-0001' }))
  })

  it('asks before it cancels a task from the row menu', async () => {
    const { onAction } = table()
    const user = userEvent.setup()

    await user.pointer({ keys: '[MouseRight]', target: rowOf('Write release notes') })
    await user.click(screen.getByRole('menuitem', { name: /^Cancel task/ }))
    expect(onAction).not.toHaveBeenCalled()

    const dialog = screen.getByRole('dialog', { name: 'Cancel T-0002?' })
    expect(document.activeElement?.textContent).toBe('Keep working')
    expect(dialog.textContent).toContain('A cancelled task cannot be started again.')
    await user.click(within(dialog).getByRole('button', { name: 'Cancel task' }))
    expect(onAction).toHaveBeenCalledWith(
      expect.objectContaining({ id: 'T-0002' }),
      expect.objectContaining({ type: 'task.cancelled' }),
    )
  })

  it('asks before it deletes a task, and says the workspace goes with it', async () => {
    const { onDelete } = table([
      makeTask({ id: 'T-0003', title: 'Keep the notes', state: 'done', workspace: '/work/T-0003' }),
    ])
    const user = userEvent.setup()

    await user.pointer({ keys: '[MouseRight]', target: rowOf('Keep the notes') })
    expect(screen.queryByRole('menuitem', { name: 'Cancel task' })).toBeNull()
    await user.click(screen.getByRole('menuitem', { name: 'Delete task' }))
    expect(onDelete).not.toHaveBeenCalled()

    const dialog = screen.getByRole('dialog', { name: 'Delete T-0003?' })
    expect(within(dialog).getByText(/worktree folder/)).toBeTruthy()
    await user.click(within(dialog).getByRole('button', { name: 'Delete task' }))
    expect(onDelete).toHaveBeenCalledWith(expect.objectContaining({ id: 'T-0003' }))
  })

  it('re-renders only the row of the task that changed', () => {
    // New handlers on every render, as the dashboard passes them.
    const tableOf = (tasks: readonly Task[]) => (
      <TaskTable
        tasks={tasks}
        mainWidth={null}
        selectedId={null}
        sort={null}
        onSort={() => {}}
        onSelect={() => {}}
        onOpen={() => {}}
        onToggleDetails={() => {}}
      />
    )
    const { rerender } = render(tableOf(TASKS))
    vi.mocked(rowLabel).mockClear()

    rerender(tableOf([makeTask({ title: 'Fix the login test' }), ...TASKS.slice(1)]))

    expect(vi.mocked(rowLabel).mock.calls.map(([task]) => task.id)).toEqual(['T-0001'])
    expect(rowOf('Fix the login test')).toBeTruthy()
  })
})
