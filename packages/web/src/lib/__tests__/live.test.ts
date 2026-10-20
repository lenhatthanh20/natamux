import type { Task } from '@natamux/core'
import { QueryClient } from '@tanstack/react-query'
import { describe, expect, it, vi } from 'vitest'
import dashboardView from '../../features/app/Dashboard.tsx?raw'
import liveParts from '../../features/app/LiveParts.tsx?raw'
import dashboardCommands from '../../features/app/useDashboardCommands.ts?raw'
import dashboardLayout from '../../features/app/useDashboardLayout.ts?raw'
import dashboard from '../../features/app/useDashboardQueries.ts?raw'
import taskMutations from '../../features/app/useTaskMutations.ts?raw'
import changesPanel from '../../features/git/ChangesPanel.tsx?raw'
import changesView from '../../features/workspaces/ChangesView.tsx?raw'
import docsTab from '../../features/workspaces/DocsTab.tsx?raw'
import type { AgentActivity, Job } from '../api'
import {
  ACTIVITY_KEY,
  applyLiveMessages,
  closeSocket,
  DIFFSTATS_EVERY_MS,
  FALLBACK_POLL_MS,
  JOBS_KEY,
  liveSocketUrl,
  PREPARING_KEY,
  parseLiveMessage,
} from '../live'
import { SHELLS_EVERY_MS } from '../task-shells'
import taskShells from '../task-shells.ts?raw'

const task: Task = {
  id: 'T-0001',
  title: 'Example',
  description: '',
  state: 'review',
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
  updatedAt: '2026-10-08T09:00:00.000Z',
}

describe('liveSocketUrl', () => {
  it('uses ws for http and wss for https', () => {
    expect(liveSocketUrl({ protocol: 'http:', host: 'localhost:5173' }, 'abc')).toBe(
      'ws://localhost:5173/api/events?token=abc',
    )
    expect(liveSocketUrl({ protocol: 'https:', host: 'app.github.dev' }, 'abc')).toBe(
      'wss://app.github.dev/api/events?token=abc',
    )
  })

  it('encodes the token', () => {
    expect(liveSocketUrl({ protocol: 'http:', host: 'localhost' }, 'a b&c')).toContain(
      'token=a%20b%26c',
    )
  })
})

describe('parseLiveMessage', () => {
  const parse = (message: unknown) => parseLiveMessage(JSON.stringify(message))

  it('reads the task of a created message', () => {
    expect(parse({ kind: 'task.created', task })).toEqual({ kind: 'task', task, logged: false })
  })

  it('reads the task of an updated message, which logged an event', () => {
    const message = { kind: 'task.updated', task, accepted: true, event: {} }
    expect(parse(message)).toEqual({ kind: 'task', task, logged: true })
  })

  it('reads the task of a changed message, which logged nothing', () => {
    expect(parse({ kind: 'task.changed', task })).toEqual({ kind: 'task', task, logged: false })
  })

  it.each([
    { kind: 'task.deleted', taskId: 'T-0001' },
    { kind: 'repos.changed' },
    { kind: 'resync' },
    { kind: 'shells.changed', taskId: 'T-0001' },
    { kind: 'workspace.dirty', taskId: 'T-0001' },
    { kind: 'workspace.preparing', taskId: 'T-0001', step: 'checkout' },
    { kind: 'workspace.preparing', taskId: 'T-0001', step: null, error: 'git worktree failed' },
  ])('reads $kind', (message) => {
    expect(parse(message)).toEqual(message)
  })

  it('reads the activity of a task as the line of each terminal and the latest line', () => {
    const activity = {
      '1': { text: 'Edit src/a.ts', at: '2026-10-08T09:00:00Z' },
      '2': { text: 'Bash: pnpm test', at: '2026-10-08T09:00:05Z' },
    }
    expect(parse({ kind: 'activity.changed', taskId: 'T-0001', activity })).toEqual({
      kind: 'activity.changed',
      taskId: 'T-0001',
      lines: { '1': 'Edit src/a.ts', '2': 'Bash: pnpm test' },
      latest: 'Bash: pnpm test',
    })
  })

  it('reads an activity that is empty as no lines', () => {
    expect(parse({ kind: 'activity.changed', taskId: 'T-0001', activity: {} })).toEqual({
      kind: 'activity.changed',
      taskId: 'T-0001',
      lines: {},
      latest: null,
    })
  })

  it('reads the list of background jobs', () => {
    const jobs: Job[] = [
      {
        id: 'job-1',
        kind: 'workspace',
        label: 'Pushing',
        taskId: 'T-0001',
        startedAt: '2026-10-21T09:00:00.000Z',
      },
      {
        id: 'job-2',
        kind: 'issue-import',
        label: 'Importing issues',
        taskId: null,
        startedAt: '2026-10-21T09:00:01.000Z',
      },
    ]
    expect(parse({ kind: 'jobs.changed', jobs })).toEqual({ kind: 'jobs.changed', jobs })
    expect(parse({ kind: 'jobs.changed', jobs: [] })).toEqual({ kind: 'jobs.changed', jobs: [] })
  })

  it('parses the text once', () => {
    const json = vi.spyOn(JSON, 'parse')
    parseLiveMessage('{"kind":"shells.changed","taskId":"T-0001"}')
    expect(json).toHaveBeenCalledTimes(1)
    json.mockRestore()
  })

  it.each([
    ['text that is not JSON', 'not json'],
    ['JSON that is not an object', '42'],
    ['null', 'null'],
    ['a message without a kind or a task', '{"taskId":"T-0001"}'],
    ['a kind this dashboard does not know', '{"kind":"later","taskId":"T-0001"}'],
    ['a message without a task', '{"kind":"task.updated"}'],
    ['a task that is not valid', '{"task":{"id":"nope"}}'],
    ['a delete message without an id', '{"kind":"task.deleted"}'],
    ['a shells message with an id that is not text', '{"kind":"shells.changed","taskId":3}'],
    ['an activity message without an activity', '{"kind":"activity.changed","taskId":"T-0001"}'],
    ['a jobs message without jobs', '{"kind":"jobs.changed"}'],
    ['a jobs message with a job that has no label', '{"kind":"jobs.changed","jobs":[{"id":"a"}]}'],
    [
      'a step this dashboard does not know',
      '{"kind":"workspace.preparing","taskId":"T-0001","step":"later"}',
    ],
  ])('ignores %s', (_name, text) => {
    expect(parseLiveMessage(text)).toBeUndefined()
  })
})

describe('applyLiveMessages', () => {
  const client = () => new QueryClient({ defaultOptions: { queries: { retry: false } } })

  const activity: AgentActivity = {
    byTask: { 'T-0001': 'Edit src/a.ts', 'T-0002': 'Bash: pnpm test' },
    byTerminal: { 'T-0001': { '1': 'Edit src/a.ts' }, 'T-0002': { '1': 'Bash: pnpm test' } },
  }

  const changed = (taskId: string, lines: Record<string, string>, latest: string | null) =>
    ({ kind: 'activity.changed', taskId, lines, latest }) as const

  it('writes the activity of the tasks and keeps the other tasks as they were', () => {
    const queryClient = client()
    queryClient.setQueryData(ACTIVITY_KEY, activity)

    applyLiveMessages(queryClient, [
      changed('T-0001', { '1': 'Read a.ts' }, 'Read a.ts'),
      changed('T-0003', { '2': 'Grep' }, 'Grep'),
    ])

    const next = queryClient.getQueryData<AgentActivity>(ACTIVITY_KEY)
    expect(next?.byTask).toEqual({
      'T-0001': 'Read a.ts',
      'T-0002': 'Bash: pnpm test',
      'T-0003': 'Grep',
    })
    expect(next?.byTerminal['T-0001']).toEqual({ '1': 'Read a.ts' })
    // The same object, so a part that shows T-0002 sees no change.
    expect(next?.byTerminal['T-0002']).toBe(activity.byTerminal['T-0002'])
  })

  it('drops a task whose agents do nothing now', () => {
    const queryClient = client()
    queryClient.setQueryData(ACTIVITY_KEY, activity)

    applyLiveMessages(queryClient, [changed('T-0001', {}, null)])

    const next = queryClient.getQueryData<AgentActivity>(ACTIVITY_KEY)
    expect(next?.byTask).toEqual({ 'T-0002': 'Bash: pnpm test' })
    expect(Object.keys(next?.byTerminal ?? {})).toEqual(['T-0002'])
  })

  it('leaves the activity to its first read when it was not read yet', () => {
    const queryClient = client()

    applyLiveMessages(queryClient, [changed('T-0001', { '1': 'Grep' }, 'Grep')])

    expect(queryClient.getQueryData(ACTIVITY_KEY)).toBeUndefined()
  })

  it('reads the shells of that task again, once however many messages came', () => {
    const queryClient = client()
    const invalidate = vi.spyOn(queryClient, 'invalidateQueries')

    applyLiveMessages(queryClient, [
      { kind: 'shells.changed', taskId: 'T-0001' },
      { kind: 'shells.changed', taskId: 'T-0001' },
    ])

    expect(invalidate).toHaveBeenCalledTimes(3)
    expect(invalidate).toHaveBeenCalledWith({ queryKey: ['task-shells', 'T-0001'] })
    expect(invalidate).toHaveBeenCalledWith({ queryKey: ['ended-shells', 'T-0001'] })
    // The open shells, which the Terminal tab and the palette show.
    expect(invalidate).toHaveBeenCalledWith({ queryKey: ['terminals', 'T-0001'] })
  })

  it('writes the step of each workspace being made, and drops one that ended', () => {
    const queryClient = client()
    queryClient.setQueryData(PREPARING_KEY, { 'T-0001': 'fetch', 'T-0002': 'checkout' })

    applyLiveMessages(queryClient, [
      { kind: 'workspace.preparing', taskId: 'T-0001', step: 'checkout' },
      { kind: 'workspace.preparing', taskId: 'T-0002', step: null },
      { kind: 'workspace.preparing', taskId: 'T-0003', step: 'fetch' },
    ])

    expect(queryClient.getQueryData(PREPARING_KEY)).toEqual({
      'T-0001': 'checkout',
      'T-0003': 'fetch',
    })
  })

  it('leaves the steps alone before they were first read', () => {
    const queryClient = client()

    applyLiveMessages(queryClient, [
      { kind: 'workspace.preparing', taskId: 'T-0001', step: 'fetch' },
    ])

    expect(queryClient.getQueryData(PREPARING_KEY)).toBeUndefined()
  })

  it('reads the files and the sizes of a changed workspace again, not its shells', () => {
    const queryClient = client()
    const invalidate = vi.spyOn(queryClient, 'invalidateQueries')

    applyLiveMessages(queryClient, [
      { kind: 'workspace.dirty', taskId: 'T-0001' },
      { kind: 'workspace.dirty', taskId: 'T-0002' },
    ])

    const keys = invalidate.mock.calls.map(([filters]) => filters?.queryKey)
    const workspaceKeys = ['changes', 'file-diff', 'graph', 'stage-files', 'tree', 'workspace-file']
    for (const id of ['T-0001', 'T-0002']) {
      for (const key of workspaceKeys) {
        expect(keys).toContainEqual([key, id])
      }
    }
    // The daemon sends shells.changed for what the shells show.
    expect(keys.filter((key) => key?.[0] === 'task-shells')).toEqual([])
    // The sizes of all tasks come in one answer, read once.
    expect(keys.filter((key) => key?.[0] === 'diffstats')).toEqual([['diffstats']])
  })

  it('reads the sizes at most once per interval, and once more after the last change', () => {
    vi.useFakeTimers()
    const queryClient = client()
    const invalidate = vi.spyOn(queryClient, 'invalidateQueries')
    const sizeReads = () =>
      invalidate.mock.calls.filter(([filters]) => filters?.queryKey?.[0] === 'diffstats').length
    const dirty = (taskId: string) =>
      applyLiveMessages(queryClient, [{ kind: 'workspace.dirty', taskId }])

    dirty('T-0001')
    expect(sizeReads()).toBe(1)

    // Ten busy tasks, each changing every 10 seconds, for 40 seconds.
    for (let second = 0; second < 40; second += 1) {
      dirty(`T-${String(second % 10).padStart(4, '0')}`)
      vi.advanceTimersByTime(1000)
    }
    expect(sizeReads()).toBe(3)
    // The files of each task are still read at each change of that task.
    expect(invalidate).toHaveBeenCalledWith({ queryKey: ['changes', 'T-0009'] })

    vi.advanceTimersByTime(DIFFSTATS_EVERY_MS)
    expect(sizeReads()).toBe(4)
    vi.advanceTimersByTime(DIFFSTATS_EVERY_MS * 3)
    expect(sizeReads()).toBe(4)

    // After a quiet time, the next change is read at once.
    dirty('T-0001')
    expect(sizeReads()).toBe(5)
    vi.useRealTimers()
  })

  it('reads the log of a task again when it logged an event, once however many came', () => {
    const queryClient = client()
    const invalidate = vi.spyOn(queryClient, 'invalidateQueries')

    applyLiveMessages(queryClient, [
      { kind: 'task', task, logged: true },
      { kind: 'task', task, logged: true },
    ])

    const keys = invalidate.mock.calls.map(([filters]) => filters?.queryKey)
    expect(keys).toEqual([
      ['events', 'T-0001'],
      ['stage-events', 'T-0001'],
      ['stage-files', 'T-0001'],
      ['question-events', 'T-0001'],
    ])
  })

  it('reads nothing again when only the time of a task moved', () => {
    const queryClient = client()
    const invalidate = vi.spyOn(queryClient, 'invalidateQueries')

    applyLiveMessages(queryClient, [{ kind: 'task', task, logged: false }])

    expect(invalidate).not.toHaveBeenCalled()
  })

  it('keeps the last list of background jobs of the batch', () => {
    const queryClient = client()
    const invalidate = vi.spyOn(queryClient, 'invalidateQueries')
    const job: Job = {
      id: 'job-1',
      kind: 'workspace',
      label: 'Pushing',
      taskId: 'T-0001',
      startedAt: '2026-10-21T09:00:00.000Z',
    }

    applyLiveMessages(queryClient, [{ kind: 'jobs.changed', jobs: [job] }])
    expect(queryClient.getQueryData(JOBS_KEY)).toEqual([job])

    applyLiveMessages(queryClient, [
      { kind: 'jobs.changed', jobs: [job] },
      { kind: 'jobs.changed', jobs: [] },
    ])
    expect(queryClient.getQueryData(JOBS_KEY)).toEqual([])
    expect(invalidate).not.toHaveBeenCalled()
  })

  it('reads everything again after a resync', () => {
    const queryClient = client()
    const invalidate = vi.spyOn(queryClient, 'invalidateQueries')

    applyLiveMessages(queryClient, [
      { kind: 'shells.changed', taskId: 'T-0001' },
      { kind: 'resync' },
    ])

    expect(invalidate).toHaveBeenCalledTimes(1)
    expect(invalidate).toHaveBeenCalledWith()
  })
})

describe('closeSocket', () => {
  function fake(readyState: number) {
    return {
      readyState,
      close: vi.fn(),
      onopen: null as (() => void) | null,
      onmessage: vi.fn() as unknown,
      onclose: vi.fn() as unknown,
      onerror: vi.fn() as unknown,
    }
  }

  it('closes an open socket at once', () => {
    const socket = fake(1)
    closeSocket(socket as unknown as WebSocket)
    expect(socket.close).toHaveBeenCalledTimes(1)
  })

  it('waits for a socket that is still connecting, so the browser logs no warning', () => {
    const socket = fake(0)
    closeSocket(socket as unknown as WebSocket)
    expect(socket.close).not.toHaveBeenCalled()
    expect(socket.onmessage).toBeNull()
    expect(socket.onclose).toBeNull()
    socket.onopen?.()
    expect(socket.close).toHaveBeenCalledTimes(1)
  })
})

// The live feed brings activity, shells and workspace changes, so their queries only poll to catch
// what it missed.
describe('the fallback poll', () => {
  it('runs once a minute', () => {
    expect(FALLBACK_POLL_MS).toBe(60_000)
  })

  const sources = Object.entries({ changesPanel, changesView, dashboard, docsTab })

  it.each(sources)('is the only poll of %s', (_name, source) => {
    // A number here would be a poll of its own, such as the old 3, 5, 10 or 15 seconds.
    expect(source.match(/refetchInterval: \d/g)).toBeNull()
    expect(source).toContain('FALLBACK_POLL_MS')
  })

  // The dashboard polls only in useDashboardQueries, never in its other parts.
  it.each(
    Object.entries({ dashboardView, liveParts, dashboardCommands, dashboardLayout, taskMutations }),
  )('has no poll of its own in %s', (_name, source) => {
    expect(source.match(/refetchInterval: \d/g)).toBeNull()
  })

  // One poll per task row, so it waits longer than the others.
  it('waits longer for the shells of each task', () => {
    expect(taskShells.match(/refetchInterval: \d/g)).toBeNull()
    expect(SHELLS_EVERY_MS).toBeGreaterThan(FALLBACK_POLL_MS)
  })
})
