import type { Task } from '@natamux/core'
import { cleanup, render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { rowLabel } from '../../../lib/tasks'
import { TaskBoard } from '../TaskBoard'

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

function board(tasks: readonly Task[] = TASKS) {
  render(
    <TaskBoard
      tasks={tasks}
      selectedId={null}
      stateFilter={null}
      showFinished={false}
      onSelect={() => {}}
      onOpen={() => {}}
    />,
  )
}

const column = (name: string) => screen.getByRole('region', { name })

describe('TaskBoard', () => {
  it('folds empty columns into rails, so the columns with tasks fit in view', () => {
    board()

    const failed = column('Failed')
    expect(within(failed).getByRole('button', { name: /Fix flaky login test/ })).toBeTruthy()
    const ready = column('Ready')
    expect(within(ready).getByRole('button', { name: /Write release notes/ })).toBeTruthy()
    const rail = within(column('In review')).getByRole('button', {
      name: 'Expand In review, 0 tasks',
    })
    expect(rail.getAttribute('aria-expanded')).toBe('false')
    expect(screen.queryByText('Nothing here')).toBeNull()
  })

  it('opens an empty column from its rail, and folds it again', async () => {
    board()
    const user = userEvent.setup()

    await user.click(screen.getByRole('button', { name: 'Expand In review, 0 tasks' }))
    expect(within(column('In review')).getByText('Nothing here')).toBeTruthy()

    await user.click(screen.getByRole('button', { name: 'Collapse In review' }))
    expect(screen.getByRole('button', { name: 'Expand In review, 0 tasks' })).toBeTruthy()
  })

  it('folds a column with tasks when the user asks', async () => {
    board()
    const user = userEvent.setup()

    await user.click(screen.getByRole('button', { name: 'Collapse Failed' }))

    expect(screen.queryByRole('button', { name: /Fix flaky login test/ })).toBeNull()
    await user.click(screen.getByRole('button', { name: 'Expand Failed, 1 task' }))
    expect(screen.getByRole('button', { name: /Fix flaky login test/ })).toBeTruthy()
  })

  it('names the agent as the pickers do, not by its stored id', () => {
    board()

    const card = screen.getByRole('button', { name: /Fix flaky login test/ })
    expect(within(card).getByText('Claude Code')).toBeTruthy()
    expect(within(card).queryByText('claude-code')).toBeNull()
  })

  describe('delete all finished tasks', () => {
    const FINISHED = [
      makeTask({ id: 'T-0003', title: 'Ship the icon', state: 'done' }),
      makeTask({ id: 'T-0004', title: 'Old spike', state: 'done' }),
      ...TASKS,
    ]
    const finishedBoard = (onDeleteTasks = vi.fn(), deletingTasks = false) => {
      render(
        <TaskBoard
          tasks={FINISHED}
          selectedId={null}
          stateFilter={null}
          showFinished={true}
          onSelect={() => {}}
          onOpen={() => {}}
          onDeleteTasks={onDeleteTasks}
          deletingTasks={deletingTasks}
        />,
      )
      return onDeleteTasks
    }

    it('shows the button only on a finished column with tasks', () => {
      finishedBoard()

      expect(
        within(column('Done')).getByRole('button', { name: 'Delete all done tasks' }),
      ).toBeTruthy()
      expect(screen.queryByRole('button', { name: 'Delete all cancelled tasks' })).toBeNull()
      expect(screen.queryByRole('button', { name: /^Delete all (failed|ready) tasks$/ })).toBeNull()
    })

    it('has no button when the dashboard gives no handler', () => {
      render(
        <TaskBoard
          tasks={FINISHED}
          selectedId={null}
          stateFilter={null}
          showFinished={true}
          onSelect={() => {}}
          onOpen={() => {}}
        />,
      )

      expect(screen.queryByRole('button', { name: 'Delete all done tasks' })).toBeNull()
    })

    it('lists the tasks of the column, and deletes them after the confirm', async () => {
      const onDeleteTasks = finishedBoard()
      const user = userEvent.setup()

      await user.click(screen.getByRole('button', { name: 'Delete all done tasks' }))
      const dialog = screen.getByRole('dialog', { name: 'Delete 2 done tasks?' })
      expect(within(dialog).getByText('Ship the icon')).toBeTruthy()
      expect(within(dialog).getByText('Old spike')).toBeTruthy()
      expect(within(dialog).queryByText('Write release notes')).toBeNull()

      await user.click(within(dialog).getByRole('button', { name: 'Delete' }))

      expect(onDeleteTasks).toHaveBeenCalledWith(['T-0003', 'T-0004'])
      expect(screen.queryByRole('dialog')).toBeNull()
    })

    it('deletes nothing when the dialog is cancelled', async () => {
      const onDeleteTasks = finishedBoard()
      const user = userEvent.setup()

      await user.click(screen.getByRole('button', { name: 'Delete all done tasks' }))
      await user.click(screen.getByRole('button', { name: 'Cancel' }))

      expect(onDeleteTasks).not.toHaveBeenCalled()
      expect(screen.queryByRole('dialog')).toBeNull()
    })

    it('turns the button off while a delete runs', () => {
      finishedBoard(vi.fn(), true)

      const button = screen.getByRole('button', { name: 'Delete all done tasks' })
      expect((button as HTMLButtonElement).disabled).toBe(true)
    })
  })

  it('re-renders only the card of the task that changed', () => {
    // New handlers on every render, as the dashboard passes them.
    const boardOf = (tasks: readonly Task[]) => (
      <TaskBoard
        tasks={tasks}
        selectedId={null}
        stateFilter={null}
        showFinished={false}
        onSelect={() => {}}
        onOpen={() => {}}
      />
    )
    const { rerender } = render(boardOf(TASKS))
    vi.mocked(rowLabel).mockClear()

    rerender(boardOf([makeTask({ title: 'Fix the login test' }), ...TASKS.slice(1)]))

    expect(vi.mocked(rowLabel).mock.calls.map(([task]) => task.id)).toEqual(['T-0001'])
    expect(screen.getByRole('button', { name: /^Fix the login test,/ })).toBeTruthy()
  })
})
