import type { Task } from '@natamux/core'
import { cleanup, render, screen, within } from '@testing-library/react'
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
  const handlers = { onSelect: vi.fn(), onOpen: vi.fn(), onToggleDetails: vi.fn() }
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
})
