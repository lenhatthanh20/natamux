import type { Task } from '@natamux/core'
import { act, cleanup, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import type { ComponentProps, FunctionComponent } from 'react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { App } from './App'
import type { WorkspaceRow } from './components/WorkspaceRow'
import { initialViewData, useViewStore } from './lib/view-store'

type RowProps = ComponentProps<typeof WorkspaceRow>

// How often each part rendered. Test-only: the stand-ins below count, the real parts do not.
const renders = vi.hoisted(() => ({ dashboard: 0, rows: new Map<string, number>() }))

vi.mock('./components/Dashboard', async (importOriginal) => {
  const actual = await importOriginal<typeof import('./components/Dashboard')>()
  return {
    ...actual,
    Dashboard: (props: ComponentProps<typeof actual.Dashboard>) => {
      renders.dashboard += 1
      return actual.Dashboard(props)
    },
  }
})

// The row is memo(render). The stand-in is the same memo around a render that counts, so it
// renders exactly when the real row would.
vi.mock('./components/WorkspaceRow', async (importOriginal) => {
  const actual = await importOriginal<typeof import('./components/WorkspaceRow')>()
  const { memo } = await import('react')
  const inner = (actual.WorkspaceRow as unknown as { type: FunctionComponent<RowProps> }).type
  return {
    ...actual,
    WorkspaceRow: memo((props: RowProps) => {
      renders.rows.set(props.task.id, (renders.rows.get(props.task.id) ?? 0) + 1)
      return inner(props)
    }),
  }
})

// The dashboard loads the terminal's and the diff's code when idle; xterm and Monaco need a real
// browser.
vi.mock('./components/TerminalView', () => ({ default: () => null }))
vi.mock('./components/MonacoDiff', () => ({ default: () => null }))

class FakeSocket {
  static instances: FakeSocket[] = []
  readyState = 1
  onopen: (() => void) | null = null
  onmessage: ((event: { data: unknown }) => void) | null = null
  onclose: (() => void) | null = null

  constructor(readonly url: string) {
    FakeSocket.instances.push(this)
  }

  close(): void {}
}

function makeTask(id: string, title: string): Task {
  return {
    id,
    title,
    description: '',
    state: 'running',
    priority: 'normal',
    labels: [],
    agent: null,
    branch: `natamux/${id.toLowerCase()}`,
    workspace: `/work/${id}`,
    pr: null,
    prStatus: null,
    stage: 'implement',
    stages: ['implement'],
    lanes: {},
    stageSince: null,
    backgroundSince: null,
    workflowStages: [],
    context: null,
    issue: null,
    parentId: null,
    repo: null,
    dependsOn: [],
    createdAt: '2026-10-08T09:00:00.000Z',
    // Recent, so no task turns quiet while the test runs.
    updatedAt: new Date().toISOString(),
  }
}

const TASKS = [
  makeTask('T-0001', 'Parse dates'),
  makeTask('T-0002', 'Add export'),
  makeTask('T-0003', 'Fix login'),
]

const AT = '2026-10-08T09:00:00Z'

// The shells the daemon sends for a task, when not the one shell every task has.
const shellsOf = new Map<string, unknown>()

function json(data: unknown): Response {
  return new Response(JSON.stringify(data), { status: 200 })
}

// A small stand-in for the daemon: only what the dashboard reads when it opens.
function installDaemon() {
  const fetchMock = vi.fn(async (path: string) => {
    const route = path.split('?')[0] ?? path
    if (route === '/api/tasks') {
      return json({ tasks: TASKS })
    }
    if (route === '/api/repos') {
      return json({ repos: [{ id: 'default', name: 'natamux', path: '/work/natamux' }] })
    }
    if (route === '/api/activity') {
      return json({ activity: { 'T-0001': { '1': { text: 'Read a.ts', at: AT } } } })
    }
    if (route === '/api/diffstats') {
      return json({ stats: {} })
    }
    if (route === '/api/templates') {
      return json({ templates: [] })
    }
    if (route === '/api/git/branches') {
      return json({ branches: ['main'], current: 'main' })
    }
    const shells = /^\/api\/tasks\/(T-\d+)\/terminals$/.exec(route)
    if (shells !== null) {
      return json(shellsOf.get(shells[1] ?? '') ?? { terminals: ['1'], agents: [], lanes: {} })
    }
    return new Response(JSON.stringify({ message: 'Not found' }), { status: 404 })
  })
  vi.stubGlobal('fetch', fetchMock)
  return fetchMock
}

// Waits until no request has started for a while, so late answers do not count as renders.
async function settle(fetchMock: ReturnType<typeof installDaemon>) {
  let calls = -1
  while (calls !== fetchMock.mock.calls.length) {
    calls = fetchMock.mock.calls.length
    await act(() => new Promise((resolve) => setTimeout(resolve, 50)))
  }
}

function send(message: unknown) {
  act(() => {
    for (const socket of FakeSocket.instances) {
      socket.onmessage?.({ data: JSON.stringify(message) })
    }
  })
}

async function openDashboard() {
  const fetchMock = installDaemon()
  const user = userEvent.setup()
  render(<App />)
  await user.type(screen.getByLabelText('Daemon token'), 'secret')
  await user.click(screen.getByRole('button', { name: 'Connect' }))
  const sidebar = await screen.findByRole('navigation', { name: 'Workspaces' })
  await within(sidebar).findByText('Read a.ts')
  act(() => FakeSocket.instances[0]?.onopen?.())
  await settle(fetchMock)
  return { sidebar, fetchMock }
}

let mark: ReturnType<typeof vi.fn>

beforeEach(() => {
  window.localStorage.clear()
  useViewStore.setState({ ...initialViewData, pinned: [] })
  FakeSocket.instances = []
  vi.stubGlobal('WebSocket', FakeSocket)
  mark = vi.fn()
  // jsdom may have no marks; an own property also hides a real one until it is deleted.
  Object.defineProperty(performance, 'mark', { configurable: true, writable: true, value: mark })
  renders.dashboard = 0
  renders.rows.clear()
  shellsOf.clear()
})

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
  Reflect.deleteProperty(performance, 'mark')
})

describe('render scope', () => {
  it('re-renders only the row of the task whose activity changed', async () => {
    const { sidebar } = await openDashboard()
    renders.dashboard = 0
    renders.rows.clear()

    send({
      kind: 'activity.changed',
      taskId: 'T-0002',
      activity: { '1': { text: 'Bash: pnpm test', at: AT } },
    })
    await within(sidebar).findByText('Bash: pnpm test')

    expect(renders.dashboard).toBe(0)
    expect([...renders.rows.keys()]).toEqual(['T-0002'])
  })

  it('does not re-render the dashboard for a part of the view it does not read', async () => {
    await openDashboard()
    renders.dashboard = 0

    act(() => useViewStore.getState().requestShell('T-0001', '2'))
    act(() => useViewStore.getState().clearShellRequest())

    expect(renders.dashboard).toBe(0)
  })

  it('marks the first task list once', async () => {
    const { fetchMock } = await openDashboard()
    expect(mark.mock.calls.filter(([name]) => name === 'natamux/first-tasks')).toHaveLength(1)

    // A task that changes, and a list read again after a reconnect, do not mark again.
    send({ kind: 'task.updated', task: { ...TASKS[0], title: 'Parse all dates' } })
    await screen.findAllByText('Parse all dates')
    act(() => FakeSocket.instances[0]?.onclose?.())
    act(() => FakeSocket.instances[0]?.onopen?.())
    await settle(fetchMock)

    expect(mark.mock.calls.filter(([name]) => name === 'natamux/first-tasks')).toHaveLength(1)
  })

  it('marks an empty first task list too', async () => {
    const fetchMock = installDaemon()
    fetchMock.mockImplementation(async (path: string) =>
      path === '/api/tasks' ? json({ tasks: [] }) : json({ repos: [], templates: [], stats: {} }),
    )
    const user = userEvent.setup()
    render(<App />)
    await user.type(screen.getByLabelText('Daemon token'), 'secret')
    await user.click(screen.getByRole('button', { name: 'Connect' }))

    await waitFor(() => expect(mark).toHaveBeenCalledWith('natamux/first-tasks'))
  })
})

describe('render scope of the open workspace, the view and the shells', () => {
  it('re-renders only the open row when the activity of the open workspace changes', async () => {
    const { fetchMock } = await openDashboard()
    act(() => useViewStore.getState().openChanges('T-0002', 'terminal'))
    await settle(fetchMock)
    renders.dashboard = 0
    renders.rows.clear()

    send({
      kind: 'activity.changed',
      taskId: 'T-0002',
      activity: { '1': { text: 'Bash: pnpm test', at: AT } },
    })
    await waitFor(() => expect(renders.rows.get('T-0002')).toBeGreaterThan(0))

    expect(renders.dashboard).toBe(0)
    expect([...renders.rows.keys()]).toEqual(['T-0002'])
  })

  it('does not re-render the dashboard when the tab kept for a task changes', async () => {
    await openDashboard()
    renders.dashboard = 0

    act(() => useViewStore.getState().rememberTab('T-0001', 'history'))

    expect(useViewStore.getState().tabByTask['T-0001']).toBe('history')
    expect(renders.dashboard).toBe(0)
  })

  it('re-renders only the row of the task whose shells changed', async () => {
    const { fetchMock } = await openDashboard()
    renders.dashboard = 0
    renders.rows.clear()
    // The one shell of T-0002 ends, so its row has nothing left to unfold.
    shellsOf.set('T-0002', { terminals: [], agents: [], lanes: {} })

    send({ kind: 'shells.changed', taskId: 'T-0002' })
    await waitFor(() => expect(renders.rows.get('T-0002')).toBeGreaterThan(0))
    await settle(fetchMock)

    expect(renders.dashboard).toBe(0)
    expect([...renders.rows.keys()]).toEqual(['T-0002'])
  })

  it('keeps every row as it is when the dashboard renders for something else', async () => {
    await openDashboard()
    renders.dashboard = 0
    renders.rows.clear()

    // The dashboard reads this and gives the sidebar new callbacks; the rows get stable ones.
    act(() => useViewStore.getState().setDetailsView('details'))

    expect(renders.dashboard).toBeGreaterThan(0)
    expect(renders.rows.size).toBe(0)
  })
})
