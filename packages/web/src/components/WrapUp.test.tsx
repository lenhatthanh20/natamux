import type { EndedShell, Task } from '@natamux/core'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { cleanup, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import type { ComponentProps } from 'react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import type { TaskShells } from '../lib/api'
import type { GitFacts } from '../lib/finished'
import { WrapUp } from './WrapUp'

afterEach(cleanup)

const PR = 'https://github.com/o/r/pull/310'
const task: Pick<Task, 'id' | 'state' | 'pr' | 'prStatus' | 'branch'> = {
  id: 'T-0058',
  state: 'done',
  pr: PR,
  prStatus: { state: 'merged', checks: 'passing' },
  branch: 'natamux/t-0058-icons',
}
const clean: GitFacts = {
  uncommitted: 0,
  graph: { ahead: 0, base: 'main', push: { remote: 'tracked', unpushed: 0, unpulled: 0 } },
}
const saved: EndedShell[] = [
  {
    name: '1',
    agent: 'claude-code',
    stage: 'implement',
    endedAt: '2026-10-14T17:07:00.000Z',
    text: 'All checks pass.',
  },
  {
    name: '6',
    agent: null,
    stage: null,
    endedAt: '2026-10-14T17:07:00.000Z',
    text: 'error: bad flag',
  },
]
const NONE: TaskShells = { terminals: [], lanes: {}, programs: {} }

type Options = Partial<ComponentProps<typeof WrapUp>> & {
  ended?: EndedShell[]
  shells?: TaskShells
}

function wrapUp({ ended = saved, shells = NONE, ...props }: Options = {}) {
  const handlers = {
    onAction: vi.fn(),
    onOpenShells: vi.fn(),
    closeTerminal: vi.fn(async () => undefined),
  }
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  render(
    <QueryClientProvider client={client}>
      <WrapUp
        task={task}
        facts={clean}
        paths={[]}
        pushLabel="Push"
        loadEndedShells={async () => ended}
        loadShells={async () => shells}
        {...handlers}
        {...props}
      />
    </QueryClientProvider>,
  )
  return handlers
}

describe('WrapUp', () => {
  it('lists what is left, every row ticked for a merged task', () => {
    wrapUp()

    const checks = screen.getByRole('list', { name: 'Wrap-up' })
    const rows = within(checks).getAllByRole('listitem').map((row) => row.textContent)
    expect(rows[0]).toContain('All changes are committed')
    expect(rows[1]).toContain('Pushed to origin')
    expect(rows[2]).toContain('Pull request #310 merged into main')
    expect(rows[3]).toContain('The branch is merged.')
    expect(within(checks).getByRole('link', { name: 'Open' }).getAttribute('href')).toBe(PR)
  })

  it('runs the action of a row', async () => {
    const { onAction } = wrapUp({
      facts: { ...clean, uncommitted: 2 },
      paths: ['a.ts', 'b.ts'],
    })

    expect(screen.getByText('a.ts, b.ts')).toBeTruthy()
    await userEvent.setup().click(screen.getByRole('button', { name: 'View changes' }))
    expect(onAction).toHaveBeenCalledWith('view-changes')
  })

  it('shows the last screen of a shell that ended with the task', async () => {
    wrapUp()
    const user = userEvent.setup()

    expect(await screen.findByText('Shell 1')).toBeTruthy()
    expect(screen.getByText(/^Claude Code · Implement · ended/)).toBeTruthy()
    expect(screen.getByText(/^Plain shell · ended/)).toBeTruthy()
    expect(screen.queryByText('All checks pass.')).toBeNull()

    const show = screen.getAllByRole('button', { name: 'Show last screen' })[0] as HTMLElement
    await user.click(show)
    expect(screen.getByText('All checks pass.')).toBeTruthy()
    expect(show.getAttribute('aria-expanded')).toBe('true')
    await user.click(screen.getByRole('button', { name: 'Hide last screen' }))
    expect(screen.queryByText('All checks pass.')).toBeNull()
  })

  it('says when no screen was kept', async () => {
    wrapUp({ ended: [] })

    expect(await screen.findByText('No shell screens were kept for this task.')).toBeTruthy()
  })

  it('lists a shell that still runs, and stops it after asking', async () => {
    const shells = { terminals: ['7'], lanes: {}, programs: { '7': 'node' } }
    const { closeTerminal, onOpenShells } = wrapUp({ shells })
    const user = userEvent.setup()

    expect(await screen.findByText('Shell 7')).toBeTruthy()
    expect(screen.getByText('node · running')).toBeTruthy()
    await user.click(screen.getByRole('button', { name: 'Open Shell 7' }))
    expect(onOpenShells).toHaveBeenCalledTimes(1)

    await user.click(screen.getByRole('button', { name: 'Stop Shell 7' }))
    await user.click(screen.getByRole('button', { name: 'Stop' }))
    await waitFor(() => expect(closeTerminal).toHaveBeenCalledWith('7'))
  })

  it('opens a new shell', async () => {
    const { onOpenShells } = wrapUp()

    await userEvent.setup().click(screen.getByRole('button', { name: 'Open a new shell' }))
    expect(onOpenShells).toHaveBeenCalledTimes(1)
  })
})
