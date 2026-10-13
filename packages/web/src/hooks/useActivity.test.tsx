import type { DiffStat } from '@natamux/core'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { act, cleanup, renderHook } from '@testing-library/react'
import type { ReactNode } from 'react'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import type { AgentActivity } from '../lib/api'
import { ACTIVITY_KEY, applyLiveMessages, DIFFSTATS_KEY } from '../lib/live'
import { useActivity, useDiffStats } from './useActivity'

const ACTIVITY: AgentActivity = {
  byTask: { 'T-0001': 'Edit src/a.ts', 'T-0002': 'Bash: pnpm test' },
  byTerminal: { 'T-0001': { '1': 'Edit src/a.ts' }, 'T-0002': { '1': 'Bash: pnpm test' } },
}

const changed = (taskId: string, lines: Record<string, string>, latest: string | null) =>
  ({ kind: 'activity.changed', taskId, lines, latest }) as const

let client: QueryClient

beforeEach(() => {
  client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
})

afterEach(() => {
  cleanup()
  client.clear()
})

// Renders a reader and counts its renders.
function reader<T>(read: () => T) {
  const count = { renders: 0 }
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  )
  const view = renderHook(
    () => {
      count.renders += 1
      return read()
    },
    { wrapper },
  )
  return { view, count }
}

describe('useActivity', () => {
  it('renders a reader again only when its own task changes', () => {
    client.setQueryData(ACTIVITY_KEY, ACTIVITY)
    const { view, count } = reader(() => useActivity((all) => all.byTask['T-0001']))
    expect(view.result.current).toBe('Edit src/a.ts')
    expect(count.renders).toBe(1)

    act(() => applyLiveMessages(client, [changed('T-0002', { '1': 'Grep' }, 'Grep')]))
    expect(count.renders).toBe(1)

    act(() => applyLiveMessages(client, [changed('T-0001', { '1': 'Read a.ts' }, 'Read a.ts')]))
    expect(view.result.current).toBe('Read a.ts')
    expect(count.renders).toBe(2)
  })

  it('keeps the lines of a terminal as the same object while another task changes', () => {
    client.setQueryData(ACTIVITY_KEY, ACTIVITY)
    const { view, count } = reader(() => useActivity((all) => all.byTerminal['T-0001']))
    const before = view.result.current

    act(() => applyLiveMessages(client, [changed('T-0002', {}, null)]))

    expect(view.result.current).toBe(before)
    expect(count.renders).toBe(1)
  })

  it('gives nothing, and does not throw, for a task that is gone', () => {
    client.setQueryData(ACTIVITY_KEY, ACTIVITY)
    const { view } = reader(() => useActivity((all) => all.byTerminal['T-0001']?.['1']))
    expect(view.result.current).toBe('Edit src/a.ts')

    act(() => applyLiveMessages(client, [changed('T-0001', {}, null)]))

    expect(view.result.current).toBeUndefined()
  })

  it('gives nothing before the first read, and loads nothing itself', () => {
    const { view } = reader(() => useActivity((all) => all.byTask['T-0001']))

    expect(view.result.current).toBeUndefined()
    // No observer: the reader made no query, so nothing runs without a queryFn.
    expect(client.getQueryCache().find({ queryKey: ACTIVITY_KEY })).toBeUndefined()
  })

  it('shows the activity once the first read comes', () => {
    const { view } = reader(() => useActivity((all) => all.byTask['T-0002']))

    act(() => client.setQueryData(ACTIVITY_KEY, ACTIVITY))

    expect(view.result.current).toBe('Bash: pnpm test')
  })
})

describe('useDiffStats', () => {
  const stats: Record<string, DiffStat> = { 'T-0001': { added: 3, removed: 1, files: 2 } }

  it('reads the sizes of the cache, and not the activity', () => {
    client.setQueryData(DIFFSTATS_KEY, stats)
    const { view, count } = reader(() => useDiffStats())
    expect(view.result.current).toBe(stats)

    act(() => client.setQueryData(ACTIVITY_KEY, ACTIVITY))
    expect(count.renders).toBe(1)

    const next = { ...stats, 'T-0002': { added: 1, removed: 0, files: 1 } }
    act(() => client.setQueryData(DIFFSTATS_KEY, next))
    expect(view.result.current).toEqual(next)
    expect(count.renders).toBe(2)
  })
})
