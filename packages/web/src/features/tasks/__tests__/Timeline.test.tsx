import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { cleanup, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import type { StoredEvent } from '../../../lib/api'
import { Timeline } from '../Timeline'

// Counts the grouping of the events, to see whether a render works the lines out again.
const grouped = vi.hoisted(() => ({ calls: 0 }))
vi.mock('../task-events', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../task-events')>()
  return {
    ...actual,
    groupEvents: (events: readonly StoredEvent[]) => {
      grouped.calls += 1
      return actual.groupEvents(events)
    },
  }
})

// Edits by the user and turns of the agent, one after the other, so no two group into one line.
function events(count: number): StoredEvent[] {
  return Array.from(
    { length: count },
    (_, index): StoredEvent => ({
      seq: index + 1,
      accepted: true,
      event: {
        taskId: 'T-0001',
        source: index % 2 === 0 ? 'human' : 'agent',
        type: index % 2 === 0 ? 'task.edited' : 'agent.finished',
        at: new Date(Date.UTC(2026, 9, 8, 9, 0, index)).toISOString(),
        payload: {},
      },
    }),
  )
}

let client: QueryClient

function timeline(loadEvents: () => Promise<StoredEvent[]>) {
  client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  const element = () => (
    <QueryClientProvider client={client}>
      <Timeline taskId="T-0001" loadEvents={loadEvents} />
    </QueryClientProvider>
  )
  const view = render(element())
  return { rerender: () => view.rerender(element()) }
}

// Not by role: working out the role of a thousand rows is slow in jsdom.
const lines = () => [...screen.getByRole('list').querySelectorAll('li')]

afterEach(() => {
  cleanup()
  client.clear()
  grouped.calls = 0
  window.localStorage.clear()
})

describe('Timeline', () => {
  it('lists 1,000 events newest first, in order', async () => {
    timeline(async () => events(1000))

    await screen.findAllByText('Agent finished its turn', { exact: false })
    const shown = lines()
    expect(shown).toHaveLength(1000)
    // Event 1,000 is a turn of the agent, event 1 an edit by the user.
    expect(shown[0]?.textContent).toContain('Agent finished its turn · Agent')
    expect(shown[1]?.textContent).toContain('Details edited · You')
    expect(shown[999]?.textContent).toContain('Details edited · You')
  })

  it('works the lines out once per read, not on every render', async () => {
    const { rerender } = timeline(async () => events(10))
    await screen.findAllByText('Details edited', { exact: false })
    const calls = grouped.calls
    expect(calls).toBeGreaterThan(0)

    rerender()
    rerender()

    expect(grouped.calls).toBe(calls)
    expect(lines()[0]?.textContent).toContain('Agent finished its turn')
  })

  it('says so when there is no activity yet', async () => {
    timeline(async () => [])
    expect(await screen.findByText('No activity yet.')).toBeTruthy()
  })
})
