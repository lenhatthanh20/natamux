import type { Task } from '@natamux/core'
import { cleanup, render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it } from 'vitest'
import { TaskBoard } from './TaskBoard'

afterEach(cleanup)

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
})
