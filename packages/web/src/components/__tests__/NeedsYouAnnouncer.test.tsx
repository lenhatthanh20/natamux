import type { Task } from '@natamux/core'
import { cleanup, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it } from 'vitest'
import { NeedsYouAnnouncer } from '../NeedsYouAnnouncer'

afterEach(cleanup)

function makeTask(overrides: Partial<Task> = {}): Task {
  return {
    id: 'T-0001',
    title: 'Fix the token refresh race in the login flow',
    description: '',
    state: 'needs-input',
    priority: 'normal',
    labels: [],
    agent: null,
    branch: null,
    workspace: null,
    pr: null,
    prStatus: null,
    stage: 'implement',
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
    createdAt: '2026-10-08T09:00:00.000Z',
    updatedAt: '2026-10-08T09:55:00.000Z',
    ...overrides,
  }
}

const announcer = (waiting: Task[]) => <NeedsYouAnnouncer waiting={waiting} />

describe('NeedsYouAnnouncer', () => {
  it('says nothing for the tasks that wait when the page opens', () => {
    render(announcer([makeTask()]))

    expect(screen.getByRole('status').textContent).toBe('')
  })

  it('names a task that starts to wait, politely', () => {
    const first = makeTask()
    const view = render(announcer([first]))

    view.rerender(announcer([first, makeTask({ id: 'T-0002', title: 'Add CSV export' })]))

    const status = screen.getByRole('status')
    expect(status.textContent).toBe('Add CSV export needs you')
    expect(status.getAttribute('aria-live')).toBe('polite')
  })

  it('names every task that starts to wait at once', () => {
    const first = makeTask()
    const view = render(announcer([first]))

    view.rerender(
      announcer([
        first,
        makeTask({ id: 'T-0002', title: 'Add CSV export' }),
        makeTask({ id: 'T-0003', title: 'Fix login' }),
      ]),
    )

    expect(screen.getByRole('status').textContent).toBe(
      'Add CSV export needs you. Fix login needs you',
    )
  })

  it('says nothing new when a task stops waiting', () => {
    const first = makeTask()
    const view = render(announcer([first, makeTask({ id: 'T-0002', title: 'Add CSV export' })]))

    view.rerender(announcer([first]))

    expect(screen.getByRole('status').textContent).toBe('')
  })

  it('names a task again when it stops waiting and then waits again', () => {
    const first = makeTask()
    const later = makeTask({ id: 'T-0002', title: 'Ship docs', state: 'blocked' })
    const view = render(announcer([first]))

    const other = makeTask({ id: 'T-0003', title: 'Fix login' })
    view.rerender(announcer([first, later]))
    view.rerender(announcer([first, other]))
    expect(screen.getByRole('status').textContent).toBe('Fix login needs you')
    view.rerender(announcer([first, other, later]))

    expect(screen.getByRole('status').textContent).toBe('Ship docs needs you')
  })
})
