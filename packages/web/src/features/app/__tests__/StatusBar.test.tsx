import { act, cleanup, render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import type { ComponentProps } from 'react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import type { Job } from '../../../lib/api'
import { StatusBar } from '../StatusBar'

afterEach(cleanup)

const push: Job = {
  id: 'job-1',
  kind: 'workspace',
  label: 'Pushing',
  taskId: 'T-0135',
  startedAt: new Date(Date.now() - 12_000).toISOString(),
}
const importing: Job = {
  id: 'job-2',
  kind: 'issue-import',
  label: 'Importing issues',
  taskId: null,
  startedAt: new Date().toISOString(),
}

type Props = ComponentProps<typeof StatusBar>

function props(overrides: Partial<Props> = {}): Props {
  return {
    liveStatus: 'live',
    running: 0,
    needsYou: 0,
    jobs: [],
    onOpenTask: vi.fn(),
    onFilterRunning: vi.fn(),
    onNeedsYou: vi.fn(),
    onRepos: vi.fn(),
    onImport: vi.fn(),
    notifications: { permission: 'granted', askPermission: vi.fn() },
    onShortcuts: vi.fn(),
    onSettings: vi.fn(),
    onSignOut: vi.fn(),
    width: 1400,
    ...overrides,
  }
}

describe('the background jobs in the status bar', () => {
  it('shows nothing with no job', () => {
    render(<StatusBar {...props()} />)
    expect(screen.queryByText(/background job/)).toBeNull()
    expect(document.querySelector('[data-tip="Show background jobs"]')).toBeNull()
  })

  it('names one job with its task', () => {
    render(<StatusBar {...props({ jobs: [push] })} />)
    const trigger = screen.getByRole('button', { name: 'Pushing · T-0135' })
    expect(trigger.getAttribute('data-tip')).toBe('Show background jobs')
    expect(trigger.getAttribute('aria-haspopup')).toBe('menu')
  })

  it('counts two jobs, and lists both with their task and time', async () => {
    render(<StatusBar {...props({ jobs: [push, importing] })} />)
    await userEvent.setup().click(screen.getByRole('button', { name: '2 background jobs' }))

    const rows = within(screen.getByRole('menu')).getAllByRole('menuitem')
    expect(rows.map((row) => row.textContent)).toEqual([
      expect.stringMatching(/^PushingT-0135 · \d+s$/),
      expect.stringMatching(/^Importing issues\d+s$/),
    ])
    expect(rows[1]?.hasAttribute('disabled')).toBe(true)
  })

  it('opens the task of a row', async () => {
    const onOpenTask = vi.fn()
    render(<StatusBar {...props({ jobs: [push, importing], onOpenTask })} />)
    const user = userEvent.setup()
    await user.click(screen.getByRole('button', { name: '2 background jobs' }))
    await user.click(screen.getByRole('menuitem', { name: 'Pushing' }))
    expect(onOpenTask).toHaveBeenCalledWith('T-0135')
  })

  it('keeps the list open while jobs end, and closes it with the last one', async () => {
    const { rerender } = render(<StatusBar {...props({ jobs: [push, importing] })} />)
    await userEvent.setup().click(screen.getByRole('button', { name: '2 background jobs' }))

    act(() => rerender(<StatusBar {...props({ jobs: [importing] })} />))
    expect(within(screen.getByRole('menu')).getAllByRole('menuitem')).toHaveLength(1)

    act(() => rerender(<StatusBar {...props({ jobs: [] })} />))
    expect(screen.queryByRole('menu')).toBeNull()
  })

  // Test spec C14: focus must not fall to the page when the menu goes away with the last job.
  it('moves focus to the bar when the last job ends while its menu has focus', async () => {
    const { rerender } = render(<StatusBar {...props({ jobs: [push] })} />)
    await userEvent.setup().click(screen.getByRole('button', { name: 'Pushing · T-0135' }))
    expect(document.activeElement).toBe(screen.getByRole('menuitem', { name: 'Pushing' }))

    act(() => rerender(<StatusBar {...props({ jobs: [] })} />))

    expect(screen.queryByRole('menu')).toBeNull()
    expect(document.activeElement).not.toBe(document.body)
    expect(document.activeElement).toBe(screen.getByRole('button', { name: '0 running' }))
  })

  it('leaves focus on the page after a click outside the menu closed it', async () => {
    const { rerender } = render(<StatusBar {...props({ jobs: [push, importing] })} />)
    const user = userEvent.setup()
    await user.click(screen.getByRole('button', { name: '2 background jobs' }))
    await user.click(document.body)
    expect(screen.queryByRole('menu')).toBeNull()
    expect(document.activeElement).toBe(document.body)

    act(() => rerender(<StatusBar {...props({ jobs: [importing], running: 3 })} />))
    expect(document.activeElement).toBe(document.body)
    act(() => rerender(<StatusBar {...props({ jobs: [], running: 2 })} />))
    expect(document.activeElement).toBe(document.body)
  })

  it('leaves focus alone when it was not in the bar', () => {
    const { rerender } = render(<StatusBar {...props({ jobs: [push] })} />)
    act(() => rerender(<StatusBar {...props({ jobs: [] })} />))
    expect(document.activeElement).toBe(document.body)
  })

  it('is a spinner with a name under 1024 px', () => {
    render(<StatusBar {...props({ jobs: [push], width: 900 })} />)
    const trigger = screen.getByRole('button', { name: 'Pushing · T-0135' })
    expect(trigger.getAttribute('aria-label')).toBe('Pushing · T-0135')
    expect(trigger.textContent).toBe('')
    expect(screen.queryByRole('button', { name: 'More status items' })).not.toBeNull()
  })
})
