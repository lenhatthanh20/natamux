import { applyTaskEvent, type EventType, type Task } from '@helmsman/core'
import { act, cleanup, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { App } from './App'
import { initialViewData, useViewStore } from './lib/view-store'

function makeTask(overrides: Partial<Task> = {}): Task {
  return {
    id: 'T-0001',
    title: 'Example',
    description: '',
    state: 'draft',
    priority: 'normal',
    labels: [],
    agent: null,
    branch: null,
    workspace: null,
    pr: null,
    dependsOn: [],
    createdAt: '2026-10-08T09:00:00.000Z',
    updatedAt: '2026-10-08T09:00:00.000Z',
    ...overrides,
  }
}

class FakeSocket {
  static instances: FakeSocket[] = []
  onopen: (() => void) | null = null
  onmessage: ((event: { data: unknown }) => void) | null = null
  onclose: (() => void) | null = null

  constructor(readonly url: string) {
    FakeSocket.instances.push(this)
  }

  close(): void {
    this.onclose?.()
  }
}

type Call = { method: string; path: string; authorization: string | null }

function json(data: unknown, status = 200): Response {
  return new Response(JSON.stringify(data), { status })
}

// A small stand-in for the daemon. It applies events with the real state machine.
function installDaemon(initial: Task[]) {
  let tasks = [...initial]
  const calls: Call[] = []

  const fetchMock = vi.fn(async (path: string, init?: RequestInit) => {
    const method = init?.method ?? 'GET'
    calls.push({
      method,
      path,
      authorization: new Headers(init?.headers).get('authorization'),
    })
    const body: { title?: string; type?: EventType } =
      typeof init?.body === 'string' ? JSON.parse(init.body) : {}

    if (path === '/api/tasks' && method === 'GET') {
      return json({ tasks })
    }
    if (path === '/api/tasks' && method === 'POST') {
      const task = makeTask({ id: 'T-0099', title: body.title ?? '' })
      tasks = [task, ...tasks]
      return json({ task }, 201)
    }
    const eventPath = /^\/api\/tasks\/(T-\d+)\/events$/.exec(path)
    const taskId = eventPath?.[1]
    const target = tasks.find((task) => task.id === taskId)
    if (method === 'POST' && target && body.type) {
      const applied = applyTaskEvent(target.state, {
        taskId: target.id,
        source: 'human',
        type: body.type,
        at: '2026-10-08T12:00:00.000Z',
        payload: {},
      })
      const updated = applied.accepted ? { ...target, state: applied.state } : target
      tasks = tasks.map((task) => (task.id === updated.id ? updated : task))
      return json({ task: updated, accepted: applied.accepted })
    }
    return json({ message: 'Not found' }, 404)
  })

  vi.stubGlobal('fetch', fetchMock)
  return { calls }
}

async function signIn(user: ReturnType<typeof userEvent.setup>) {
  await user.type(screen.getByLabelText('Daemon token'), 'secret')
  await user.click(screen.getByRole('button', { name: 'Connect' }))
}

const reviewTask = makeTask({ id: 'T-0001', title: 'Fix timezone bug', state: 'review' })
const runningTask = makeTask({ id: 'T-0002', title: 'Add export', state: 'running' })

beforeEach(() => {
  window.localStorage.clear()
  useViewStore.setState(initialViewData)
  FakeSocket.instances = []
  vi.stubGlobal('WebSocket', FakeSocket)
})

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
})

describe('App', () => {
  it('asks for the token first', () => {
    installDaemon([])
    render(<App />)
    expect(screen.getByLabelText('Daemon token')).toBeTruthy()
  })

  it('shows the tasks and who needs the user after signing in', async () => {
    const { calls } = installDaemon([reviewTask, runningTask])
    const user = userEvent.setup()
    render(<App />)

    await signIn(user)

    expect(await screen.findByRole('heading', { name: '1 task needs you' })).toBeTruthy()
    const table = screen.getByRole('table')
    expect(within(table).getByText('Fix timezone bug')).toBeTruthy()
    expect(within(table).getByText('Add export')).toBeTruthy()
    expect(calls[0]).toMatchObject({ path: '/api/tasks', authorization: 'Bearer secret' })
  })

  it('says nothing needs the user when no task waits', async () => {
    installDaemon([runningTask])
    const user = userEvent.setup()
    render(<App />)

    await signIn(user)

    expect(await screen.findByRole('heading', { name: 'Nothing needs you.' })).toBeTruthy()
  })

  it('goes back to the token form when the daemon rejects the token', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => json({ error: 'unauthorized' }, 401)),
    )
    const user = userEvent.setup()
    render(<App />)

    await signIn(user)

    expect(await screen.findByText(/rejected that token/)).toBeTruthy()
    expect(window.localStorage.length).toBe(0)
  })

  it('approves a task in review from its details', async () => {
    const { calls } = installDaemon([reviewTask, runningTask])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('heading', { name: '1 task needs you' })

    await user.click(within(screen.getByRole('table')).getByRole('button', { name: /Fix timezone bug/ }))
    const details = screen.getByRole('complementary', { name: 'Details of T-0001' })
    await user.click(within(details).getByRole('button', { name: /Approve/ }))

    expect(await screen.findByRole('heading', { name: 'Nothing needs you.' })).toBeTruthy()
    expect(calls.some((call) => call.method === 'POST' && call.path === '/api/tasks/T-0001/events')).toBe(true)
  })

  it('adds a task from the title field', async () => {
    const { calls } = installDaemon([runningTask])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')

    await user.type(screen.getByLabelText('New task title'), 'Write the docs')
    await user.click(screen.getByRole('button', { name: 'Add task' }))

    expect(await within(screen.getByRole('table')).findByText('Write the docs')).toBeTruthy()
    expect(calls.some((call) => call.method === 'POST' && call.path === '/api/tasks')).toBe(true)
  })

  it('moves the selection with j and opens the details with Enter', async () => {
    installDaemon([reviewTask, runningTask])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')

    await user.keyboard('j')
    await user.keyboard('{Enter}')

    expect(screen.getByRole('complementary', { name: /Details of T-000/ })).toBeTruthy()
  })

  it('shows a change that arrives over the live connection', async () => {
    installDaemon([reviewTask, runningTask])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('heading', { name: '1 task needs you' })

    const socket = FakeSocket.instances[0]
    expect(socket?.url).toContain('/api/events?token=secret')
    act(() => {
      socket?.onmessage?.({
        data: JSON.stringify({
          kind: 'task.updated',
          task: { ...runningTask, state: 'needs-input' },
        }),
      })
    })

    await waitFor(() => expect(screen.getByRole('heading', { name: '2 tasks need you' })).toBeTruthy())
  })
})
