import type { Task } from '@natamux/core'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { act, cleanup, renderHook } from '@testing-library/react'
import type { ReactNode } from 'react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { AgentActivity } from '../../../lib/api'
import { ACTIVITY_KEY } from '../../../lib/live'
import { BATCH_FALLBACK_MS } from '../task-batch'
import { useLiveTasks } from '../useLiveTasks'

class FakeSocket {
  static instances: FakeSocket[] = []
  readyState = 1
  onopen: (() => void) | null = null
  onmessage: ((event: { data: unknown }) => void) | null = null
  onclose: (() => void) | null = null
  onerror: (() => void) | null = null

  constructor(readonly url: string) {
    FakeSocket.instances.push(this)
  }

  close(): void {}

  // A message from the daemon.
  receive(message: unknown): void {
    act(() => {
      this.onmessage?.({ data: typeof message === 'string' ? message : JSON.stringify(message) })
    })
  }
}

const task: Task = {
  id: 'T-0001',
  title: 'Example',
  description: '',
  state: 'running',
  priority: 'normal',
  labels: [],
  agent: null,
  branch: null,
  workspace: null,
  pr: null,
  prStatus: null,
  stage: 'implement',
  stages: ['implement'],
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
  updatedAt: '2026-10-08T09:00:00.000Z',
}

let queryClient: QueryClient

beforeEach(() => {
  queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  FakeSocket.instances = []
  vi.stubGlobal('WebSocket', FakeSocket)
  // No frame runs here, as in a hidden tab: the timer writes each batch.
  vi.stubGlobal('requestAnimationFrame', () => 1)
  vi.stubGlobal('cancelAnimationFrame', () => {})
  vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] })
})

afterEach(() => {
  cleanup()
  queryClient.clear()
  vi.useRealTimers()
  vi.unstubAllGlobals()
  vi.restoreAllMocks()
})

function live() {
  const handlers = { onTask: vi.fn(), onRepos: vi.fn(), onDeleted: vi.fn() }
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
  )
  const view = renderHook(
    () => useLiveTasks('secret', handlers.onTask, handlers.onRepos, handlers.onDeleted),
    { wrapper },
  )
  const socket = FakeSocket.instances[0]
  if (socket === undefined) {
    throw new Error('The hook opened no socket.')
  }
  return { ...handlers, socket, view }
}

const flush = () => act(() => vi.advanceTimersByTime(BATCH_FALLBACK_MS))

describe('useLiveTasks', () => {
  it('hands tasks, deleted tasks and changed projects to the dashboard at once', () => {
    const { socket, onTask, onRepos, onDeleted } = live()

    socket.receive({ kind: 'task.updated', task: { ...task, id: 'T-0003' } })
    socket.receive({ kind: 'task.deleted', taskId: 'T-0002' })
    socket.receive({ kind: 'repos.changed' })

    expect(onTask).toHaveBeenCalledWith(expect.objectContaining({ id: 'T-0003' }))
    expect(onDeleted).toHaveBeenCalledWith('T-0002')
    expect(onRepos).toHaveBeenCalledTimes(1)
  })

  it('writes a burst of activity into the cache once', () => {
    const { socket } = live()
    const before: AgentActivity = { byTask: {}, byTerminal: {} }
    queryClient.setQueryData(ACTIVITY_KEY, before)
    const write = vi.spyOn(queryClient, 'setQueryData')
    const at = '2026-10-08T09:00:00Z'

    for (const text of ['Read a.ts', 'Edit a.ts', 'Bash: pnpm test']) {
      const activity = { '1': { text, at } }
      socket.receive({ kind: 'activity.changed', taskId: 'T-0001', activity })
    }
    expect(write).not.toHaveBeenCalled()
    flush()

    expect(write).toHaveBeenCalledTimes(1)
    expect(queryClient.getQueryData(ACTIVITY_KEY)).toEqual({
      byTask: { 'T-0001': 'Bash: pnpm test' },
      byTerminal: { 'T-0001': { '1': 'Bash: pnpm test' } },
    })
  })

  it('reads the shells and the workspace of the task again', () => {
    const { socket } = live()
    const invalidate = vi.spyOn(queryClient, 'invalidateQueries')

    socket.receive({ kind: 'shells.changed', taskId: 'T-0001' })
    socket.receive({ kind: 'workspace.dirty', taskId: 'T-0002' })
    flush()

    const keys = invalidate.mock.calls.map(([filters]) => filters?.queryKey)
    expect(keys).toContainEqual(['task-shells', 'T-0001'])
    expect(keys).toContainEqual(['changes', 'T-0002'])
    expect(keys).toContainEqual(['diffstats'])
  })

  it('reads the log of a task again for a logged event, not for a change of its time only', () => {
    const { socket, onTask } = live()
    const invalidate = vi.spyOn(queryClient, 'invalidateQueries')
    const later = { ...task, updatedAt: '2026-10-08T09:00:30.000Z' }

    socket.receive({ kind: 'task.changed', task: later })
    flush()
    expect(onTask).toHaveBeenCalledWith(later)
    expect(invalidate).not.toHaveBeenCalled()

    socket.receive({ kind: 'task.updated', task: later, accepted: true, event: {} })
    flush()
    const keys = invalidate.mock.calls.map(([filters]) => filters?.queryKey)
    expect(keys).toContainEqual(['events', 'T-0001'])
    expect(keys).toContainEqual(['stage-events', 'T-0001'])
  })

  it('reads everything again after a resync', () => {
    const { socket } = live()
    const invalidate = vi.spyOn(queryClient, 'invalidateQueries')

    socket.receive({ kind: 'resync' })
    flush()

    expect(invalidate).toHaveBeenCalledWith()
  })

  it('ignores what it cannot read, and keeps listening', () => {
    const { socket, onTask, onRepos, onDeleted } = live()
    const invalidate = vi.spyOn(queryClient, 'invalidateQueries')

    socket.receive('not json')
    socket.receive({ kind: 'later', taskId: 'T-0001' })
    flush()
    expect(onTask).not.toHaveBeenCalled()
    expect(onDeleted).not.toHaveBeenCalled()
    expect(onRepos).not.toHaveBeenCalled()
    expect(invalidate).not.toHaveBeenCalled()

    socket.receive({ kind: 'repos.changed' })
    expect(onRepos).toHaveBeenCalledTimes(1)
  })

  it('reads the lists again on the first connect, and everything after a reconnect', () => {
    const { socket, view } = live()
    const invalidate = vi.spyOn(queryClient, 'invalidateQueries')

    act(() => socket.onopen?.())
    expect(view.result.current).toBe('live')
    expect(invalidate.mock.calls.map(([filters]) => filters?.queryKey)).toEqual([
      ['tasks'],
      ['repos'],
    ])

    invalidate.mockClear()
    act(() => socket.onclose?.())
    expect(view.result.current).toBe('reconnecting')
    act(() => socket.onopen?.())
    expect(invalidate).toHaveBeenCalledTimes(1)
    expect(invalidate).toHaveBeenCalledWith()
  })

  it('writes the messages of one frame together, and nothing before the frame', () => {
    const { socket } = live()
    queryClient.setQueryData(ACTIVITY_KEY, { byTask: {}, byTerminal: {} })
    const write = vi.spyOn(queryClient, 'setQueryData')
    const invalidate = vi.spyOn(queryClient, 'invalidateQueries')
    const activity = { '1': { text: 'Grep', at: '2026-10-08T09:00:00Z' } }

    socket.receive({ kind: 'shells.changed', taskId: 'T-0001' })
    socket.receive({ kind: 'activity.changed', taskId: 'T-0001', activity })
    socket.receive({ kind: 'shells.changed', taskId: 'T-0001' })
    expect(write).not.toHaveBeenCalled()
    expect(invalidate).not.toHaveBeenCalled()
    flush()

    expect(write).toHaveBeenCalledTimes(1)
    // Once per key, however many messages came: the shells, the open shells, and the screens a
    // finished task kept.
    expect(invalidate).toHaveBeenCalledTimes(3)
    expect(invalidate).toHaveBeenCalledWith({ queryKey: ['task-shells', 'T-0001'] })
    expect(invalidate).toHaveBeenCalledWith({ queryKey: ['terminals', 'T-0001'] })
    expect(invalidate).toHaveBeenCalledWith({ queryKey: ['ended-shells', 'T-0001'] })
  })

  it('opens a new socket after a drop, and reads everything again once it opens', () => {
    const { socket, view } = live()
    act(() => socket.onopen?.())
    act(() => socket.onclose?.())
    expect(FakeSocket.instances).toHaveLength(1)

    act(() => vi.advanceTimersByTime(1000))
    const next = FakeSocket.instances[1]
    expect(next?.url).toBe(socket.url)
    const invalidate = vi.spyOn(queryClient, 'invalidateQueries')
    act(() => next?.onopen?.())

    expect(view.result.current).toBe('live')
    expect(invalidate).toHaveBeenCalledTimes(1)
    expect(invalidate).toHaveBeenCalledWith()
  })

  it('drops the queued messages when it goes', () => {
    const { socket, view } = live()
    queryClient.setQueryData(ACTIVITY_KEY, { byTask: {}, byTerminal: {} })
    const write = vi.spyOn(queryClient, 'setQueryData')

    socket.receive({ kind: 'activity.changed', taskId: 'T-0001', activity: {} })
    view.unmount()
    flush()

    expect(write).not.toHaveBeenCalled()
  })
})
