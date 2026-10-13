import { QueryClient, QueryClientProvider, useQuery } from '@tanstack/react-query'
import { act, cleanup, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import type { TaskShells } from './api'
import { applyLiveMessages } from './live'
import { agentLanes, SHELLS_EVERY_MS, taskShellsQuery } from './task-shells'

const SHELLS: TaskShells = {
  terminals: ['1', '2'],
  lanes: { '1': { stage: 'implement', state: 'running' } },
  programs: { '1': 'node', '2': 'zsh' },
}

type Load = (taskId: string) => Promise<TaskShells>

// As the sidebar reads the shells.
function Sidebar({ load }: { load: Load }) {
  const shells = useQuery(taskShellsQuery('T-0001', load))
  return <p>Shells {shells.data?.terminals.join(' ')}</p>
}

// As the open workspace and the agents grid read them.
function Workspace({ load }: { load: Load }) {
  const lanes = useQuery({ ...taskShellsQuery('T-0001', load), select: agentLanes })
  return <p>Agents {Object.keys(lanes.data ?? {}).join(' ')}</p>
}

describe('task shells', () => {
  afterEach(() => {
    cleanup()
    vi.useRealTimers()
  })

  it('reads the shells again when the daemon says they changed, not before the poll', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true })
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
    const load = vi.fn(async (_taskId: string) => SHELLS)
    render(
      <QueryClientProvider client={client}>
        <Sidebar load={load} />
      </QueryClientProvider>,
    )
    await act(() => vi.advanceTimersByTimeAsync(10_000))
    expect(load).toHaveBeenCalledTimes(1)

    await act(async () => {
      applyLiveMessages(client, [{ kind: 'shells.changed', taskId: 'T-0001' }])
      await vi.advanceTimersByTimeAsync(0)
    })
    expect(load).toHaveBeenCalledTimes(2)
    client.clear()
  })

  it('reads the shells again after a minute without a message, not sooner', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true })
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
    const load = vi.fn(async (_taskId: string) => SHELLS)
    render(
      <QueryClientProvider client={client}>
        <Sidebar load={load} />
      </QueryClientProvider>,
    )
    await act(() => vi.advanceTimersByTimeAsync(0))
    expect(load).toHaveBeenCalledTimes(1)

    await act(() => vi.advanceTimersByTimeAsync(59_000))
    expect(load).toHaveBeenCalledTimes(1)
    await act(() => vi.advanceTimersByTimeAsync(1000))
    expect(load).toHaveBeenCalledTimes(2)
    client.clear()
  })

  it('reads a task once per poll however many parts show its shells', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true })
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
    const load = vi.fn(async (_taskId: string) => SHELLS)
    const view = render(
      <QueryClientProvider client={client}>
        <Sidebar load={load} />
      </QueryClientProvider>,
    )
    await act(() => vi.advanceTimersByTimeAsync(2000))

    // The workspace opens between two polls of the sidebar.
    view.rerender(
      <QueryClientProvider client={client}>
        <Sidebar load={load} />
        <Workspace load={load} />
      </QueryClientProvider>,
    )
    await act(() => vi.advanceTimersByTimeAsync(0))
    expect(screen.getByText('Shells 1 2')).toBeTruthy()
    expect(screen.getByText('Agents 1')).toBeTruthy()
    expect(load).toHaveBeenCalledWith('T-0001')

    load.mockClear()
    await act(() => vi.advanceTimersByTimeAsync(SHELLS_EVERY_MS * 4))
    expect(load).toHaveBeenCalledTimes(4)
    client.clear()
  })
})
