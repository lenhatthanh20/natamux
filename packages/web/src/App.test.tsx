import { applyTaskEvent, type EventType, type Task, type TaskEvent } from '@helmsman/core'
import { act, cleanup, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { createElement } from 'react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { App } from './App'
import { initialViewData, useViewStore } from './lib/view-store'

// Monaco needs a real browser, so the tests use a plain stand-in.
vi.mock('./components/MonacoDiff', () => ({
  default: ({ path, original, modified }: { path: string; original: string; modified: string }) =>
    createElement('pre', { 'aria-label': `Changes in ${path}` }, `${original} -> ${modified}`),
}))

vi.mock('./components/TerminalView', () => ({
  default: () => createElement('p', null, 'Terminal stand-in'),
}))

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
    prStatus: null,
    parentId: null,
    repo: null,
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
function installDaemon(initial: Task[], extraEvents: TaskEvent[] = []) {
  let tasks = [...initial]
  let templates: { id: string; name: string; labels: string[] }[] = []
  let repos = [{ id: 'default', name: 'helmsman', path: '/work/helmsman' }]
  const calls: Call[] = []
  const log: { accepted: boolean; event: TaskEvent }[] = [
    ...initial.map((task) => ({
      accepted: true,
      event: {
        taskId: task.id,
        source: 'system' as const,
        type: 'task.created' as const,
        at: task.createdAt,
        payload: {},
      },
    })),
    ...extraEvents.map((event) => ({ accepted: true, event })),
  ]

  const fetchMock = vi.fn(async (path: string, init?: RequestInit) => {
    const method = init?.method ?? 'GET'
    calls.push({
      method,
      path,
      authorization: new Headers(init?.headers).get('authorization'),
    })
    const body: { title?: string; type?: EventType; parentId?: string } =
      typeof init?.body === 'string' ? JSON.parse(init.body) : {}

    if (path === '/api/tasks' && method === 'GET') {
      return json({ tasks })
    }
    if (path === '/api/tasks' && method === 'POST') {
      const task = makeTask({
        id: 'T-0099',
        title: body.title ?? '',
        parentId: body.parentId ?? null,
      })
      tasks = [task, ...tasks]
      return json({ task }, 201)
    }
    const eventPath = /^\/api\/tasks\/(T-\d+)\/events$/.exec(path)
    const taskId = eventPath?.[1]
    const target = tasks.find((task) => task.id === taskId)
    if (method === 'GET' && target) {
      const events = log
        .filter((entry) => entry.event.taskId === target.id)
        .map((entry, index) => ({ seq: index + 1, ...entry }))
      return json({ events })
    }
    if (method === 'POST' && target && body.type) {
      const event: TaskEvent = {
        taskId: target.id,
        source: 'human',
        type: body.type,
        at: '2026-10-08T12:00:00.000Z',
        payload: {},
      }
      const applied = applyTaskEvent(target.state, event)
      log.push({ accepted: applied.accepted, event })
      const updated = applied.accepted
        ? { ...target, state: applied.state, updatedAt: event.at }
        : target
      tasks = tasks.map((task) => (task.id === updated.id ? updated : task))
      return json({ task: updated, accepted: applied.accepted })
    }
    const workspacePath = /^\/api\/tasks\/(T-\d+)\/workspace(\?.*)?$/.exec(path)
    const workspaceTask = tasks.find((task) => task.id === workspacePath?.[1])
    if (workspaceTask && (method === 'POST' || method === 'DELETE')) {
      const creating = method === 'POST'
      const updated: Task = {
        ...workspaceTask,
        workspace: creating ? `/work/${workspaceTask.id}` : null,
        branch: creating ? `helmsman/${workspaceTask.id.toLowerCase()}` : null,
      }
      tasks = tasks.map((task) => (task.id === updated.id ? updated : task))
      return creating ? json({ task: updated }, 201) : json({ task: updated, branchDeleted: true })
    }
    if (/^\/api\/tasks\/T-\d+\/changes$/.test(path) && method === 'GET') {
      return json({
        changes: [
          { path: 'src/a.ts', status: 'M', originalPath: null },
          { path: 'notes.md', status: 'U', originalPath: null },
        ],
      })
    }
    if (/^\/api\/tasks\/T-\d+\/changes\/file\?/.test(path) && method === 'GET') {
      const requested = new URL(path, 'http://daemon').searchParams.get('path') ?? ''
      return json({
        path: requested,
        status: 'M',
        original: 'old line',
        modified: 'new line',
        binary: false,
        tooLarge: false,
      })
    }
    if (path === '/api/templates' && method === 'GET') {
      return json({ templates })
    }
    if (path === '/api/templates' && method === 'POST') {
      const input = JSON.parse(String(init?.body))
      const template = { ...input, id: 'fix-it' }
      templates = [template]
      return json({ template }, 201)
    }
    const fromTemplate = /^\/api\/templates\/([a-z-]+)\/tasks$/.exec(path)
    if (fromTemplate && method === 'POST') {
      const task = makeTask({
        id: 'T-0077',
        title: JSON.parse(String(init?.body)).title,
        labels: templates[0]?.labels ?? [],
      })
      tasks = [...tasks, task]
      return json({ task }, 201)
    }
    if (path === '/api/git/branches' && method === 'GET') {
      return json({ branches: ['main', 'release'], current: 'main' })
    }
    if (path === '/api/repos' && method === 'GET') {
      return json({ repos })
    }
    if (path === '/api/repos' && method === 'POST') {
      const input = JSON.parse(String(init?.body))
      const repo = { id: 'web', name: input.name ?? 'web', path: input.path }
      repos = [...repos, repo]
      return json({ repo }, 201)
    }
    if (path === '/api/diffstats' && method === 'GET') {
      return json({ stats: { 'T-0001': { files: 2, added: 12, removed: 3 } } })
    }
    if (/^\/api\/tasks\/T-\d+\/terminals$/.test(path) && method === 'GET') {
      return json({ terminals: ['1'] })
    }
    if (path === '/api/github/issues' && method === 'GET') {
      return json({
        issues: [
          {
            number: 12,
            title: 'Dark mode flickers',
            body: '',
            url: 'https://github.com/o/r/issues/12',
            labels: ['ui'],
            taskId: null,
          },
        ],
      })
    }
    if (path === '/api/github/issues/import' && method === 'POST') {
      const task = makeTask({ id: 'T-0050', title: 'Dark mode flickers', labels: ['ui'] })
      tasks = [...tasks, task]
      return json({ tasks: [task] })
    }
    const editPath = /^\/api\/tasks\/(T-\d+)$/.exec(path)
    const editTask = tasks.find((task) => task.id === editPath?.[1])
    if (editTask && method === 'PATCH') {
      const updated: Task = { ...editTask, ...JSON.parse(String(init?.body)) }
      tasks = tasks.map((task) => (task.id === updated.id ? updated : task))
      return json({ task: updated })
    }
    if (/^\/api\/tasks\/T-\d+\/fan-out$/.test(path) && method === 'POST') {
      const copy = makeTask({ id: 'T-0090', title: 'Fix it (try 2)', labels: ['tries-t-0001'] })
      tasks = [...tasks, copy]
      return json({ tasks: [copy] }, 201)
    }
    if (/^\/api\/tasks\/T-\d+\/agent\/message$/.test(path) && method === 'POST') {
      return json({ task: tasks[0] })
    }
    const agentPath = /^\/api\/tasks\/(T-\d+)\/agent$/.exec(path)
    const agentTask = tasks.find((task) => task.id === agentPath?.[1])
    if (agentTask && method === 'POST') {
      const updated: Task = { ...agentTask, agent: 'claude-code' }
      tasks = tasks.map((task) => (task.id === updated.id ? updated : task))
      return json({ task: updated })
    }
    const gitPath = /^\/api\/tasks\/(T-\d+)\/(commit|push|pull-request)$/.exec(path)
    const gitTask = tasks.find((task) => task.id === gitPath?.[1])
    if (gitPath && gitTask && method === 'POST') {
      const updated: Task =
        gitPath[2] === 'pull-request'
          ? { ...gitTask, pr: 'https://github.com/o/r/pull/5' }
          : gitTask
      tasks = tasks.map((task) => (task.id === updated.id ? updated : task))
      return json({ task: updated, hash: 'c'.repeat(40) })
    }
    if (/^\/api\/tasks\/T-\d+\/graph$/.test(path) && method === 'GET') {
      const commit = { author: 'Ann', date: '2026-10-08T09:00:00.000Z', refs: [] }
      return json({
        branch: 'helmsman/t-0001',
        head: 'bbbbbbbbbb',
        base: 'main',
        ahead: 1,
        behind: 0,
        truncated: false,
        commits: [
          {
            ...commit,
            hash: 'bbbbbbbbbb',
            parents: ['aaaaaaaaaa'],
            subject: 'Fix the parser',
            refs: ['helmsman/t-0001'],
            onBranch: true,
          },
          { ...commit, hash: 'aaaaaaaaaa', parents: [], subject: 'init', onBranch: false },
        ],
      })
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
// A running task is "quiet" after ten minutes, so this one must be recent whenever the tests run.
const runningTask = makeTask({
  id: 'T-0002',
  title: 'Add export',
  state: 'running',
  updatedAt: new Date().toISOString(),
})

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

    await user.click(
      within(screen.getByRole('table')).getByRole('button', { name: /Fix timezone bug/ }),
    )
    const details = screen.getByRole('complementary', { name: 'Details of T-0001' })
    await user.click(within(details).getByRole('button', { name: /Approve/ }))

    expect(await screen.findByRole('heading', { name: 'Nothing needs you.' })).toBeTruthy()
    expect(
      calls.some((call) => call.method === 'POST' && call.path === '/api/tasks/T-0001/events'),
    ).toBe(true)
  })

  it('creates a workspace from the details and removes it after a second click', async () => {
    const { calls } = installDaemon([reviewTask])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')

    await user.click(
      within(screen.getByRole('table')).getByRole('button', { name: /Fix timezone bug/ }),
    )
    const details = screen.getByRole('complementary', { name: 'Details of T-0001' })
    await user.click(within(details).getByRole('button', { name: 'Create workspace' }))
    expect(await within(details).findByTitle('/work/T-0001')).toBeTruthy()

    await user.click(within(details).getByRole('button', { name: 'Remove workspace' }))
    expect(calls.some((call) => call.method === 'DELETE')).toBe(false)
    await user.click(within(details).getByRole('button', { name: /Click again/ }))

    expect(await within(details).findByText('Workspace removed.')).toBeTruthy()
    expect(within(details).queryByTitle('/work/T-0001')).toBeNull()
    expect(calls.some((call) => call.method === 'DELETE')).toBe(true)
  })

  it('lists what happened to a task, newest first, and adds a line after an action', async () => {
    installDaemon([reviewTask])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')

    await user.click(
      within(screen.getByRole('table')).getByRole('button', { name: /Fix timezone bug/ }),
    )
    const details = screen.getByRole('complementary', { name: 'Details of T-0001' })
    const activity = await within(details).findByRole('region', { name: 'Activity' })
    expect(within(activity).getByText(/Task created/)).toBeTruthy()

    await user.click(within(details).getByRole('button', { name: /Approve/ }))

    await waitFor(() => {
      const lines = within(activity).getAllByRole('listitem')
      expect(lines.map((line) => line.textContent)).toEqual([
        expect.stringContaining('Approved'),
        expect.stringContaining('Task created'),
      ])
    })
  })

  it('counts a running task that has gone quiet among those that need the user', async () => {
    installDaemon([
      makeTask({
        id: 'T-0003',
        title: 'Silent agent',
        state: 'running',
        updatedAt: new Date(Date.now() - 30 * 60 * 1000).toISOString(),
      }),
    ])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)

    const heading = await screen.findByRole('heading', { name: '1 task needs you' })
    const strip = heading.closest('section')
    expect(strip?.textContent).toContain('quiet')

    await user.click(
      within(screen.getByRole('table')).getByRole('button', { name: /Silent agent/ }),
    )
    const details = screen.getByRole('complementary', { name: 'Details of T-0003' })
    expect(within(details).getByText(/No sign of life for/)).toBeTruthy()
  })

  it('does not count a running task that is quiet for less than ten minutes', async () => {
    installDaemon([
      makeTask({
        id: 'T-0003',
        title: 'Busy agent',
        state: 'running',
        updatedAt: new Date(Date.now() - 5 * 60 * 1000).toISOString(),
      }),
    ])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)

    expect(await screen.findByRole('heading', { name: 'Nothing needs you.' })).toBeTruthy()
  })

  it('does not list "agent is working" lines in the activity of a task', async () => {
    installDaemon(
      [reviewTask],
      [
        {
          taskId: 'T-0001',
          source: 'agent',
          type: 'agent.active',
          at: '2026-10-08T09:30:00.000Z',
          payload: {},
        },
      ],
    )
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')

    await user.click(
      within(screen.getByRole('table')).getByRole('button', { name: /Fix timezone bug/ }),
    )
    const details = screen.getByRole('complementary', { name: 'Details of T-0001' })
    const activity = await within(details).findByRole('region', { name: 'Activity' })

    expect(await within(activity).findByText(/Task created/)).toBeTruthy()
    expect(within(activity).queryByText(/Agent is working/)).toBeNull()
  })

  it('shows the workspace of a task in its details', async () => {
    installDaemon([
      makeTask({
        id: 'T-0001',
        title: 'Fix timezone bug',
        state: 'running',
        updatedAt: new Date().toISOString(),
        workspace: '/work/.helmsman/worktrees/T-0001',
        branch: 'helmsman/t-0001-fix-timezone-bug',
      }),
    ])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')

    await user.click(
      within(screen.getByRole('table')).getByRole('button', { name: /Fix timezone bug/ }),
    )
    const details = screen.getByRole('complementary', { name: 'Details of T-0001' })

    expect(within(details).getByTitle('/work/.helmsman/worktrees/T-0001')).toBeTruthy()
    expect(within(details).getByTitle('helmsman/t-0001-fix-timezone-bug')).toBeTruthy()
  })

  it('opens the changes of a workspace, shows a file diff, and closes with Escape', async () => {
    const { calls } = installDaemon([
      makeTask({
        id: 'T-0001',
        title: 'Fix timezone bug',
        state: 'running',
        updatedAt: new Date().toISOString(),
        workspace: '/work/T-0001',
        branch: 'helmsman/t-0001',
      }),
    ])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')

    await user.click(
      within(screen.getByRole('table')).getByRole('button', { name: /Fix timezone bug/ }),
    )
    const details = screen.getByRole('complementary', { name: 'Details of T-0001' })
    await user.click(within(details).getByRole('button', { name: 'View changes' }))

    const view = await screen.findByRole('region', { name: 'Changes of T-0001' })
    expect(await within(view).findByText('2 changed files')).toBeTruthy()
    expect(await within(view).findByLabelText('Changes in src/a.ts')).toBeTruthy()
    expect(
      calls.some((call) => call.path === '/api/tasks/T-0001/changes/file?path=src%2Fa.ts'),
    ).toBe(true)

    await user.click(within(view).getByRole('button', { name: /notes\.md/ }))
    expect(await within(view).findByLabelText('Changes in notes.md')).toBeTruthy()

    await user.type(within(view).getByLabelText(/for the agent/), 'Keep the old name')
    await user.click(within(view).getByRole('button', { name: 'Send to agent' }))
    expect(await within(view).findByText('Sent to the agent.')).toBeTruthy()
    const sent = calls.find((call) => call.path === '/api/tasks/T-0001/agent/message')
    expect(sent).toBeTruthy()

    await user.type(within(view).getByLabelText('Commit message'), 'Fix the parser')
    await user.click(within(view).getByRole('button', { name: 'Commit all' }))
    expect(await within(view).findByText('Committed.')).toBeTruthy()
    expect(
      calls.some((call) => call.method === 'POST' && call.path === '/api/tasks/T-0001/commit'),
    ).toBe(true)

    await user.click(within(view).getByRole('button', { name: 'Create pull request' }))
    const link = await within(view).findByRole('link', { name: 'Open pull request' })
    expect(link.getAttribute('href')).toBe('https://github.com/o/r/pull/5')

    await user.click(within(view).getByRole('tab', { name: 'History' }))
    const commits = await within(view).findByRole('list', { name: 'Commits' })
    expect(within(commits).getByText('Fix the parser')).toBeTruthy()
    expect(within(commits).getByText('helmsman/t-0001')).toBeTruthy()
    expect(within(commits).getByText('bbbbbbb')).toBeTruthy()
    expect(within(view).getByText('1 commit ahead of main, 0 behind.')).toBeTruthy()

    await user.click(within(view).getByRole('tab', { name: 'Terminal' }))
    expect(await within(view).findByText('Terminal stand-in')).toBeTruthy()
    expect(within(view).queryByRole('button', { name: 'Refresh' })).toBeNull()
    await user.click(within(view).getByRole('button', { name: 'New shell' }))
    expect(within(view).getByRole('tab', { name: 'Shell 2' }).getAttribute('aria-selected')).toBe(
      'true',
    )

    await user.keyboard('{Escape}')
    await waitFor(() =>
      expect(screen.queryByRole('region', { name: 'Changes of T-0001' })).toBeNull(),
    )
    expect(screen.getByRole('table')).toBeTruthy()
  })

  it('starts Claude Code: makes the workspace, then opens its terminal', async () => {
    const { calls } = installDaemon([
      makeTask({ id: 'T-0001', title: 'Fix timezone bug', state: 'ready' }),
    ])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')

    await user.click(
      within(screen.getByRole('table')).getByRole('button', { name: /Fix timezone bug/ }),
    )
    const details = screen.getByRole('complementary', { name: 'Details of T-0001' })
    await user.click(within(details).getByRole('button', { name: 'Start Claude Code' }))

    const view = await screen.findByRole('region', { name: 'Changes of T-0001' })
    expect(await within(view).findByText('Terminal stand-in')).toBeTruthy()
    const posts = calls.filter((call) => call.method === 'POST').map((call) => call.path)
    expect(posts).toEqual(['/api/tasks/T-0001/workspace', '/api/tasks/T-0001/agent'])
  })

  it('edits the details of a task', async () => {
    const { calls } = installDaemon([makeTask({ id: 'T-0001', title: 'Fix timezone bug' })])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')

    await user.click(
      within(screen.getByRole('table')).getByRole('button', { name: /Fix timezone bug/ }),
    )
    const details = screen.getByRole('complementary', { name: 'Details of T-0001' })
    await user.click(within(details).getByRole('button', { name: 'Edit' }))
    const form = within(details).getByRole('form', { name: 'Edit T-0001' })
    const title = within(form).getByLabelText('Title')
    await user.clear(title)
    await user.type(title, 'Fix the clock')
    await user.type(within(form).getByLabelText('Labels, comma separated'), 'time, ui,')
    await user.click(within(form).getByRole('button', { name: 'Save' }))

    expect(await within(screen.getByRole('table')).findByText('Fix the clock')).toBeTruthy()
    const patch = calls.find((call) => call.method === 'PATCH')
    expect(patch?.path).toBe('/api/tasks/T-0001')
  })

  it('imports a GitHub issue as a task', async () => {
    const { calls } = installDaemon([runningTask])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')

    await user.click(screen.getByRole('button', { name: 'Import issues' }))
    const panel = await screen.findByRole('region', { name: 'Import GitHub issues' })
    await user.click(await within(panel).findByRole('checkbox'))
    await user.click(within(panel).getByRole('button', { name: 'Import 1 issue' }))

    expect(await screen.findByText('Imported 1 issue.')).toBeTruthy()
    expect(within(screen.getByRole('table')).getByText('Dark mode flickers')).toBeTruthy()
    expect(calls.some((call) => call.path === '/api/github/issues/import')).toBe(true)
  })

  it('shows the size of the work of a task', async () => {
    installDaemon([reviewTask])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    const table = await screen.findByRole('table')

    expect(await within(table).findByText('+12')).toBeTruthy()
    expect(within(table).getByText('−3')).toBeTruthy()
  })

  it('shows the tasks as a board, one column per state', async () => {
    installDaemon([reviewTask, runningTask])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')

    await user.click(screen.getByRole('button', { name: 'board' }))

    expect(screen.queryByRole('table')).toBeNull()
    const review = screen.getByRole('region', { name: 'In review' })
    expect(within(review).getByText('Fix timezone bug')).toBeTruthy()
    expect(screen.queryByRole('region', { name: 'Done' })).toBeNull()

    await user.click(within(review).getByRole('button', { name: /Fix timezone bug/ }))
    expect(screen.getByRole('complementary', { name: 'Details of T-0001' })).toBeTruthy()
  })

  it('shows that a task waits for another one', async () => {
    installDaemon([makeTask({ id: 'T-0001', title: 'Fix timezone bug' }), runningTask])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')

    await user.click(
      within(screen.getByRole('table')).getByRole('button', { name: /Fix timezone bug/ }),
    )
    const details = screen.getByRole('complementary', { name: 'Details of T-0001' })
    await user.click(within(details).getByRole('button', { name: 'Edit' }))
    const form = within(details).getByRole('form', { name: 'Edit T-0001' })
    await user.type(within(form).getByLabelText(/Waits for tasks/), 't-0002')
    await user.click(within(form).getByRole('button', { name: 'Save' }))

    expect(await within(screen.getByRole('table')).findByText('waits for T-0002')).toBeTruthy()
    expect(
      within(details).getByRole('button', { name: 'Start Claude Code' }).hasAttribute('disabled'),
    ).toBe(true)
  })

  it('saves a task as a template and starts a new task from it', async () => {
    installDaemon([makeTask({ id: 'T-0001', title: 'Fix it', labels: ['bug'] })])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')

    await user.click(within(screen.getByRole('table')).getByRole('button', { name: /Fix it/ }))
    const details = screen.getByRole('complementary', { name: 'Details of T-0001' })
    await user.click(within(details).getByRole('button', { name: 'Save as template' }))
    expect(await screen.findByText('Saved as the template "Fix it".')).toBeTruthy()

    await user.selectOptions(await screen.findByLabelText('Template'), 'fix-it')
    await user.type(screen.getByLabelText('New task title'), 'Fix the login')
    await user.click(screen.getByRole('button', { name: 'Add task' }))

    expect(await within(screen.getByRole('table')).findByText('Fix the login')).toBeTruthy()
  })

  it('adds a subtask from the details of a task', async () => {
    installDaemon([makeTask({ id: 'T-0001', title: 'Release 1.0' })])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')

    await user.click(within(screen.getByRole('table')).getByRole('button', { name: /Release/ }))
    const details = screen.getByRole('complementary', { name: 'Details of T-0001' })
    await user.type(within(details).getByLabelText('New subtask title'), 'Write the notes')
    await user.click(within(details).getByRole('button', { name: 'Add' }))

    const subtasks = await within(details).findByRole('region', { name: 'Subtasks' })
    expect(await within(subtasks).findByText('Write the notes')).toBeTruthy()
    expect(within(subtasks).getByText('0 of 1 done')).toBeTruthy()
    expect(within(screen.getByRole('table')).getByText('part of T-0001')).toBeTruthy()
  })

  it('fans a task out to more tries and shows them together', async () => {
    const { calls } = installDaemon([makeTask({ id: 'T-0001', title: 'Fix it' })])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')

    await user.click(within(screen.getByRole('table')).getByRole('button', { name: /Fix it/ }))
    const details = screen.getByRole('complementary', { name: 'Details of T-0001' })
    await user.selectOptions(within(details).getByLabelText('Number of tries'), '1')
    await user.click(within(details).getByRole('button', { name: 'Fan out' }))

    expect(await screen.findByText('Made 1 try.')).toBeTruthy()
    expect((screen.getByLabelText('Search tasks') as HTMLInputElement).value).toBe('tries-t-0001')
    expect(within(screen.getByRole('table')).getByText('Fix it (try 2)')).toBeTruthy()
    expect(calls.some((call) => call.path === '/api/tasks/T-0001/fan-out')).toBe(true)
  })

  it('adds a repository and makes new tasks in it', async () => {
    const { calls } = installDaemon([runningTask])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')

    await user.click(screen.getByRole('button', { name: 'Repos' }))
    const panel = await screen.findByRole('region', { name: 'Repositories' })
    await user.type(within(panel).getByLabelText('Repository folder'), '/work/web')
    await user.type(within(panel).getByLabelText('Repository name'), 'Web app')
    await user.click(within(panel).getByRole('button', { name: 'Add repository' }))

    expect(await within(panel).findByText('/work/web')).toBeTruthy()
    await user.selectOptions(await screen.findByLabelText('Repository'), 'web')
    await user.type(screen.getByLabelText('New task title'), 'Fix the header')
    await user.click(screen.getByRole('button', { name: 'Add task' }))

    await waitFor(() =>
      expect(calls.some((call) => call.method === 'POST' && call.path === '/api/tasks')).toBe(true),
    )
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

    await waitFor(() =>
      expect(screen.getByRole('heading', { name: '2 tasks need you' })).toBeTruthy(),
    )
  })
})
