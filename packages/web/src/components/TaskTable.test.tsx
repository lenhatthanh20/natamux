import type { Task } from '@natamux/core'
import { cleanup, render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { TaskTable } from './TaskTable'
import { ContextMenuHost } from './ui/Menu'

afterEach(cleanup)

const NOW = new Date('2026-10-10T08:30:00.000Z')

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
    stageSince: null,
    backgroundSince: null,
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
    onAction: vi.fn(),
    onStartAgent: vi.fn(),
    onDelete: vi.fn(),
  }
  render(
    <>
      <TaskTable
        tasks={tasks}
        now={NOW}
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

  it('asks before it cancels a task from the row menu', async () => {
    const { onAction } = table()
    const user = userEvent.setup()

    await user.pointer({ keys: '[MouseRight]', target: rowOf('Write release notes') })
    await user.click(screen.getByRole('menuitem', { name: /^Cancel task/ }))
    expect(onAction).not.toHaveBeenCalled()

    const dialog = screen.getByRole('dialog', { name: 'Cancel T-0002?' })
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
})
