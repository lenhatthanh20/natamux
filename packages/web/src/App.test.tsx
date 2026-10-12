import {
  applyTaskEvent,
  type EventType,
  LEGACY_PIPELINE,
  type Task,
  type TaskEvent,
} from '@natamux/core'
import { act, cleanup, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { createElement } from 'react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { App } from './App'
import { routeOf } from './lib/route'
import { useToasts } from './lib/toasts'
import { initialViewData, useViewStore } from './lib/view-store'

// Monaco needs a real browser, so the tests use a plain stand-in.
vi.mock('./components/MonacoDiff', () => ({
  default: ({ path, original, modified }: { path: string; original: string; modified: string }) =>
    createElement('pre', { 'aria-label': `Changes in ${path}` }, `${original} -> ${modified}`),
}))

vi.mock('./components/MonacoFile', () => ({
  default: ({ path, kind, text }: { path: string; kind: string; text: string }) =>
    createElement('pre', { 'aria-label': `New file ${path}` }, `${kind}: ${text}`),
}))

// It shows the focus token it gets, which makes the real terminal take the keyboard.
vi.mock('./components/TerminalView', () => ({
  default: ({ focusToken }: { focusToken?: number }) =>
    createElement('p', { 'data-focus-token': focusToken ?? '' }, 'Terminal stand-in'),
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
    stage: 'implement',
    stages: ['implement', 'review', 'release'],
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

type Call = { method: string; path: string; authorization: string | null; body: string | null }

function json(data: unknown, status = 200): Response {
  return new Response(JSON.stringify(data), { status })
}

// A small stand-in for the daemon. It applies events with the real state machine.
const defaultRepo = { id: 'default', name: 'natamux', path: '/work/natamux' }
// The stage files of every task in the stand-in daemon.
let stageFiles: { name: string; size: number; modifiedAt: string }[] = []
// The shells each workspace has open next to the first one.
let extraShells: string[] = []
const webRepo = { id: 'web', name: 'Web app', path: '/work/web' }

function installDaemon(
  initial: Task[],
  extraEvents: TaskEvent[] = [],
  initialRepos = [defaultRepo],
) {
  let tasks = [...initial]
  let templates: { id: string; name: string; labels: string[] }[] = []
  let repos = [...initialRepos]
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
      body: typeof init?.body === 'string' ? init.body : null,
    })
    const body: { title?: string; type?: EventType; parentId?: string } =
      typeof init?.body === 'string' ? JSON.parse(init.body) : {}

    if (path === '/api/tasks' && method === 'GET') {
      return json({ tasks })
    }
    if (path === '/api/workspaces' && method === 'POST') {
      const input = JSON.parse(String(init?.body))
      const task = makeTask({
        id: 'T-0098',
        title: input.title,
        state: 'ready',
        description: input.prompt ?? '',
        agent: input.agent ?? null,
        workspace: '/work/T-0098',
        branch: 'natamux/t-0098-new',
      })
      tasks = [task, ...tasks]
      return json({ task }, 201)
    }
    if (path === '/api/tasks' && method === 'POST') {
      const task = makeTask({
        id: `T-${String(99 + tasks.length).padStart(4, '0')}`,
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
      const applied = applyTaskEvent(
        { stage: target.stage, state: target.state, lanes: target.lanes },
        event,
        LEGACY_PIPELINE,
      )
      log.push({ accepted: applied.accepted, event })
      const updated = applied.accepted
        ? { ...target, ...applied.position, updatedAt: event.at }
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
        branch: creating ? `natamux/${workspaceTask.id.toLowerCase()}` : null,
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
      const isNew = requested === 'notes.md'
      return json({
        path: requested,
        status: isNew ? 'U' : 'M',
        original: isNew ? null : 'old line',
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
    if (path === '/api/activity' && method === 'GET') {
      const entry = { text: 'Bash: pnpm test', at: '2026-10-08T09:00:00Z' }
      return json({ activity: { 'T-0002': { '1': entry } } })
    }
    // The dashboard names the project in a query.
    const route = path.split('?')[0]
    if (route === '/api/git/branches' && method === 'GET') {
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
    const repoPath = /^\/api\/repos\/([a-z0-9-]+)$/.exec(path)
    if (repoPath && method === 'DELETE') {
      const id = repoPath[1]
      const inUse = tasks.filter((task) => task.repo === id && task.workspace !== null).length
      if (inUse > 0) {
        const message = `Remove the ${inUse} workspace of ${id} first`
        return json({ error: 'conflict', message }, 409)
      }
      repos = repos.filter((repo) => repo.id !== id)
      return new Response(null, { status: 204 })
    }
    if (path === '/api/diffstats' && method === 'GET') {
      return json({ stats: { 'T-0001': { files: 2, added: 12, removed: 3 } } })
    }
    if (/^\/api\/tasks\/T-\d+\/terminals$/.test(path) && method === 'GET') {
      const task = tasks.find((item) => path === `/api/tasks/${item.id}/terminals`)
      const terminals = ['1', ...extraShells]
      if (!task?.agent) {
        return json({ terminals, agents: [], lanes: {} })
      }
      const lanes = { '1': { stage: task.stage, state: task.state } }
      return json({ terminals, agents: ['1'], lanes })
    }
    if (route === '/api/github/issues' && method === 'GET') {
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
      // Like the daemon: a task with no workspace gets one first.
      const { agent } = JSON.parse(String(init?.body)) as { agent: string }
      const updated: Task = {
        ...agentTask,
        agent,
        workspace: agentTask.workspace ?? `/work/${agentTask.id}`,
        branch: agentTask.branch ?? `natamux/${agentTask.id.toLowerCase()}`,
      }
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
        branch: 'natamux/t-0001',
        head: 'bbbbbbbbbb',
        base: 'main',
        ahead: 1,
        behind: 0,
        push: { remote: 'tracked', unpushed: 1, unpulled: 0 },
        truncated: false,
        commits: [
          {
            ...commit,
            hash: 'bbbbbbbbbb',
            parents: ['aaaaaaaaaa'],
            subject: 'Fix the parser',
            refs: ['natamux/t-0001'],
            onBranch: true,
          },
          { ...commit, hash: 'aaaaaaaaaa', parents: [], subject: 'init', onBranch: false },
        ],
      })
    }
    // Only the user's own notes, so the Start a task dialog shows no source picker.
    if (/^\/api\/repos\/[a-z0-9-]+\/settings$/.test(path) && method === 'GET') {
      return json({
        settings: { context: { sources: [{ type: 'manual' }] } },
        found: true,
        path: '/work/natamux/.natamux.json',
      })
    }
    if (/^\/api\/tasks\/T-\d+\/files$/.test(path) && method === 'GET') {
      return json({ files: stageFiles })
    }
    if (/^\/api\/tasks\/T-\d+\/files\/spec\.md$/.test(path) && method === 'GET') {
      return json({ text: '# The spec' })
    }
    const stagePath = /^\/api\/tasks\/(T-\d+)\/stage$/.exec(path)
    const stageTask = tasks.find((task) => task.id === stagePath?.[1])
    if (stageTask && method === 'POST') {
      return json({ task: stageTask })
    }
    return json({ message: 'Not found' }, 404)
  })

  vi.stubGlobal('fetch', fetchMock)
  // Changes what the daemon holds without telling the dashboard, as another client would.
  const setTasks = (next: Task[]) => {
    tasks = next
  }
  const setRepos = (next: (typeof defaultRepo)[]) => {
    repos = next
  }
  return { calls, setTasks, setRepos }
}

// Full texts live in data-tip attributes, which the tooltip layer shows on hover.
function byTip(root: HTMLElement, text: string): Element | null {
  return root.querySelector(`[data-tip="${text}"]`)
}

// A task in review with a workspace, whose stage waits for the user.
function workspaceTask(overrides: Partial<Task> = {}): Task {
  return makeTask({
    id: 'T-0001',
    title: 'Fix timezone bug',
    state: 'review',
    stage: 'review',
    updatedAt: new Date().toISOString(),
    workspace: `/work/${overrides.id ?? 'T-0001'}`,
    branch: `natamux/${(overrides.id ?? 'T-0001').toLowerCase()}`,
    ...overrides,
  })
}

function openWorkspace(id: string) {
  act(() => {
    useViewStore.getState().select(id)
    useViewStore.getState().openChanges(id, 'terminal')
  })
}

// A measured window: the layout then follows its width, as in a browser.
function stubWindowWidth(width: number) {
  vi.stubGlobal('matchMedia', (media: string) => ({
    matches: false,
    media,
    onchange: null,
    addEventListener: () => {},
    removeEventListener: () => {},
    addListener: () => {},
    removeListener: () => {},
    dispatchEvent: () => false,
  }))
  vi.stubGlobal('innerWidth', width)
}

async function openStageFile(user: ReturnType<typeof userEvent.setup>, details: HTMLElement) {
  const spec = await within(details).findByRole('article', { name: 'Spec' })
  const file = await within(spec).findByRole('button', { name: /spec\.md/ })
  await waitFor(() => expect(file.hasAttribute('disabled')).toBe(false))
  await user.click(file)
}

async function signIn(user: ReturnType<typeof userEvent.setup>) {
  await user.type(screen.getByLabelText('Daemon token'), 'secret')
  await user.click(screen.getByRole('button', { name: 'Connect' }))
}

const reviewTask = makeTask({
  id: 'T-0001',
  title: 'Fix timezone bug',
  state: 'review',
  stage: 'review',
})
// A running task is "quiet" after ten minutes, so this one must be recent whenever the tests run.
const runningTask = makeTask({
  id: 'T-0002',
  title: 'Add export',
  state: 'running',
  updatedAt: new Date().toISOString(),
})

beforeEach(() => {
  window.localStorage.clear()
  stageFiles = []
  extraShells = []
  useViewStore.setState({ ...initialViewData, pinned: [] })
  useToasts.setState({ toasts: [] })
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

    expect(await screen.findByRole('img', { name: '1 waiting' })).toBeTruthy()
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

    await screen.findByRole('table')
    expect(screen.getByText('Nothing needs you')).toBeTruthy()
    expect(screen.queryByRole('img', { name: /waiting/ })).toBeNull()
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
    await screen.findByRole('img', { name: '1 waiting' })

    await user.click(
      within(screen.getByRole('table')).getByRole('button', { name: /Fix timezone bug/ }),
    )
    const details = screen.getByRole('complementary', { name: 'Details of T-0001' })
    await user.click(within(details).getByRole('button', { name: /Approve/ }))

    expect(await screen.findByText('Nothing needs you')).toBeTruthy()
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
    // The new workspace opens beside the details, which then show its progress first.
    const progressTab = within(details).getByRole('tab', { name: 'Progress' })
    await waitFor(() => expect(progressTab.getAttribute('aria-selected')).toBe('true'))
    await user.click(within(details).getByRole('tab', { name: 'Details' }))
    await waitFor(() => expect(byTip(details, '/work/T-0001')).toBeTruthy())

    await user.click(within(details).getByRole('button', { name: 'Remove workspace' }))
    expect(calls.some((call) => call.method === 'DELETE')).toBe(false)
    const confirm = screen.getByRole('dialog', { name: 'Remove the workspace?' })
    await user.click(within(confirm).getByRole('button', { name: 'Remove' }))

    expect(await screen.findByText('Workspace removed.')).toBeTruthy()
    expect(byTip(details, '/work/T-0001')).toBeNull()
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

    const badge = await screen.findByRole('img', { name: '1 waiting' })
    const strip = badge.closest('section')
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

    await screen.findByRole('table')
    expect(screen.getByText('Nothing needs you')).toBeTruthy()
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
        workspace: '/work/.natamux/worktrees/T-0001',
        branch: 'natamux/t-0001-fix-timezone-bug',
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

    expect(byTip(details, '/work/.natamux/worktrees/T-0001')).toBeTruthy()
    expect(byTip(details, 'natamux/t-0001-fix-timezone-bug')).toBeTruthy()
  })

  it('opens the changes of a workspace, shows a file diff, and closes with Escape', async () => {
    const { calls } = installDaemon([
      makeTask({
        id: 'T-0001',
        title: 'Fix timezone bug',
        agent: 'codex',
        state: 'running',
        updatedAt: new Date().toISOString(),
        workspace: '/work/T-0001',
        branch: 'natamux/t-0001',
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

    await user.click(within(view).getByRole('treeitem', { name: /notes\.md/ }))
    expect(await within(view).findByLabelText('New file notes.md')).toBeTruthy()

    await user.type(within(view).getByLabelText(/for the agent/), 'Keep the old name')
    await user.click(within(view).getByRole('button', { name: 'Send to agent' }))
    expect(await within(view).findByText('Sent to the agent.')).toBeTruthy()
    const sent = calls.find((call) => call.path === '/api/tasks/T-0001/agent/message')
    expect(sent).toBeTruthy()

    await user.type(within(view).getByLabelText('Commit message'), 'Fix the parser')
    await user.click(within(view).getByRole('button', { name: 'Commit all' }))
    expect(await screen.findByText('Committed 2 files.')).toBeTruthy()
    expect(
      calls.some((call) => call.method === 'POST' && call.path === '/api/tasks/T-0001/commit'),
    ).toBe(true)

    await user.click(within(view).getByRole('button', { name: 'Create pull request' }))
    const link = await within(view).findByRole('link', { name: 'Open pull request' })
    expect(link.getAttribute('href')).toBe('https://github.com/o/r/pull/5')

    await user.click(within(view).getByRole('tab', { name: 'History' }))
    const commits = await within(view).findByRole('list', { name: 'Commits' })
    expect(within(commits).getByText('Fix the parser')).toBeTruthy()
    expect(within(commits).getByText('natamux/t-0001')).toBeTruthy()
    expect(within(commits).getByText('bbbbbbb')).toBeTruthy()
    expect(within(view).getByText('1 commit ahead of main, 0 behind.')).toBeTruthy()

    await user.click(within(view).getByRole('tab', { name: 'Terminal' }))
    expect(await within(view).findByText('Terminal stand-in')).toBeTruthy()
    expect(within(view).queryByRole('button', { name: 'Refresh' })).toBeNull()
    await user.click(within(view).getByRole('button', { name: 'New shell' }))
    const agentShell = within(view).getByRole('region', { name: 'Shell 1' })
    const plainShell = within(view).getByRole('region', { name: 'Shell 2' })
    expect(await within(agentShell).findByText('Codex')).toBeTruthy()
    expect(within(plainShell).queryByText('Codex')).toBeNull()
    expect(within(plainShell).queryByText('Agent')).toBeNull()

    await user.keyboard('{Escape}')
    await waitFor(() =>
      expect(screen.queryByRole('region', { name: 'Changes of T-0001' })).toBeNull(),
    )
    expect(screen.getByRole('table')).toBeTruthy()
  })

  it('shows the question of a waiting agent, and resumes it', async () => {
    const { calls } = installDaemon(
      [
        makeTask({
          id: 'T-0001',
          title: 'Fix timezone bug',
          state: 'needs-input',
          workspace: '/work/T-0001',
          branch: 'natamux/t-0001',
        }),
      ],
      [
        {
          taskId: 'T-0001',
          source: 'agent',
          type: 'agent.asked',
          at: '2026-10-08T10:00:00.000Z',
          payload: { message: 'Which test runner?' },
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
    await user.click(within(details).getByRole('button', { name: 'View changes' }))

    const view = await screen.findByRole('region', { name: 'Changes of T-0001' })
    expect(await within(view).findByText('The agent asks: Which test runner?')).toBeTruthy()
    // Said once in the sidebar (Needs you) and once in the workspace (the banner), not in the
    // tree row; the banner stands for the state chip.
    await waitFor(() =>
      expect(screen.getAllByText('The agent asks: Which test runner?')).toHaveLength(2),
    )
    expect(
      within(screen.getByRole('region', { name: 'Needs you' })).getByText(/Which test/),
    ).toBeTruthy()
    expect(view.querySelector('.state-chip')).toBeNull()

    await user.click(within(view).getByRole('button', { name: 'Answer in terminal' }))
    expect(within(view).getByRole('tab', { name: 'Terminal' }).getAttribute('aria-selected')).toBe(
      'true',
    )

    await user.click(within(view).getByRole('button', { name: 'Resume' }))
    await waitFor(() =>
      expect(
        calls.some((call) => call.method === 'POST' && call.path === '/api/tasks/T-0001/events'),
      ).toBe(true),
    )
  })

  it('does not take the keyboard again when the Terminal tab opens later', async () => {
    installDaemon(
      [workspaceTask({ state: 'needs-input', stage: 'implement' })],
      [
        {
          taskId: 'T-0001',
          source: 'agent',
          type: 'agent.asked',
          at: '2026-10-08T10:00:00.000Z',
          payload: { message: 'Which test runner?' },
        },
      ],
    )
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')
    openWorkspace('T-0001')
    const view = await screen.findByRole('region', { name: 'Changes of T-0001' })
    const tokenOfShell = async () => {
      const pane = await within(view).findByRole('region', { name: 'Shell 1' })
      return (await within(pane).findByText('Terminal stand-in')).getAttribute('data-focus-token')
    }

    await user.click(await within(view).findByRole('button', { name: 'Answer in terminal' }))
    await waitFor(async () => expect(await tokenOfShell()).not.toBe(''))

    await user.click(within(view).getByRole('tab', { name: 'Changes' }))
    await user.click(within(view).getByRole('tab', { name: 'Terminal' }))
    expect(await tokenOfShell()).toBe('')
    // Give the shells time to be read, after which an old request would be used.
    await act(() => new Promise((resolve) => setTimeout(resolve, 50)))
    expect(await tokenOfShell()).toBe('')
  })

  it('shows the commits to push on Push, and says why Push is off when there is no branch', async () => {
    installDaemon([
      makeTask({
        id: 'T-0001',
        title: 'Fix timezone bug',
        workspace: '/work/T-0001',
        branch: 'natamux/t-0001',
      }),
      makeTask({ id: 'T-0003', title: 'No branch yet', workspace: '/work/T-0003' }),
    ])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')

    await user.click(
      within(screen.getByRole('table')).getByRole('button', { name: /Fix timezone bug/ }),
    )
    await user.click(
      within(screen.getByRole('complementary', { name: 'Details of T-0001' })).getByRole('button', {
        name: 'View changes',
      }),
    )
    const view = await screen.findByRole('region', { name: 'Changes of T-0001' })
    expect(await within(view).findByRole('button', { name: 'Push ↑1' })).toBeTruthy()

    await user.keyboard('{Escape}')
    await user.click(
      within(await screen.findByRole('table')).getByRole('button', { name: /No branch yet/ }),
    )
    await user.click(
      within(screen.getByRole('complementary', { name: 'Details of T-0003' })).getByRole('button', {
        name: 'View changes',
      }),
    )
    const bare = await screen.findByRole('region', { name: 'Changes of T-0003' })
    const push = within(bare).getByRole('button', { name: 'Push' })
    expect(push.getAttribute('aria-disabled')).toBe('true')
    expect(push.getAttribute('data-tip')).toBe('This task has no branch to push')
  })

  it('starts Claude Code in a new workspace and opens its terminal', async () => {
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
    await user.click(within(details).getByRole('button', { name: 'Start agent' }))

    const view = await screen.findByRole('region', { name: 'Changes of T-0001' })
    expect(await within(view).findByText('Terminal stand-in')).toBeTruthy()
    const posts = calls.filter((call) => call.method === 'POST').map((call) => call.path)
    expect(posts).toEqual(['/api/tasks/T-0001/agent'])
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
    const panel = await screen.findByRole('dialog', { name: 'Import GitHub issues' })
    expect(panel.textContent).toContain(
      'Open GitHub issues of this project. Each one you pick becomes a task.',
    )
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

    await user.click(screen.getByRole('radio', { name: 'Board' }))

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
      within(details).getByRole('button', { name: 'Start agent' }).hasAttribute('disabled'),
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
    await user.click(within(details).getByRole('button', { name: 'More actions for T-0001' }))
    await user.click(screen.getByRole('menuitem', { name: 'Save as template' }))
    expect(await screen.findByText('Saved as the template "Fix it".')).toBeTruthy()

    await user.selectOptions(await screen.findByLabelText('Template'), 'fix-it')
    await user.type(screen.getByLabelText('New task title'), 'Fix the login')
    await user.click(screen.getByRole('button', { name: 'Add task' }))

    expect(await within(screen.getByRole('table')).findByText('Fix the login')).toBeTruthy()
  })

  it('adds tasks typed one after another without waiting for the daemon', async () => {
    installDaemon([])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)

    const field = (await screen.findByLabelText('New task title')) as HTMLInputElement
    await user.type(field, 'First task{Enter}Second task{Enter}')

    expect(field.value).toBe('')
    const table = await screen.findByRole('table')
    expect(await within(table).findByText('First task')).toBeTruthy()
    expect(await within(table).findByText('Second task')).toBeTruthy()
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
    await user.click(within(details).getByRole('button', { name: 'More actions for T-0001' }))
    await user.click(screen.getByRole('menuitem', { name: 'Run more tries...' }))
    const dialog = screen.getByRole('dialog', { name: 'Run more tries' })
    await user.selectOptions(within(dialog).getByLabelText('Number of tries'), '1')
    await user.click(within(dialog).getByRole('button', { name: 'Run tries' }))

    expect(await screen.findByText('Made 1 try.')).toBeTruthy()
    expect((screen.getByLabelText('Search tasks') as HTMLInputElement).value).toBe('tries-t-0001')
    expect(within(screen.getByRole('table')).getByText('Fix it (try 2)')).toBeTruthy()
    expect(calls.some((call) => call.path === '/api/tasks/T-0001/fan-out')).toBe(true)
  })

  it('adds a project and makes new tasks in it', async () => {
    const { calls } = installDaemon([runningTask])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')

    await user.click(screen.getByRole('button', { name: 'Projects' }))
    const panel = await screen.findByRole('dialog', { name: 'Projects' })
    expect(panel.textContent).toContain(
      "Git repositories on the daemon's machine. The workspace of each task is a git worktree of one of them.",
    )
    expect(within(panel).getByLabelText('Project folder').getAttribute('placeholder')).toBe(
      '/path/to/the/git/repository',
    )
    await user.type(within(panel).getByLabelText('Project folder'), '/work/web')
    await user.type(within(panel).getByLabelText('Project name'), 'Web app')
    await user.click(within(panel).getByRole('button', { name: 'Add project' }))

    expect(await within(panel).findByText('/work/web')).toBeTruthy()
    await user.selectOptions(await screen.findByLabelText('Project'), 'web')
    await user.type(screen.getByLabelText('New task title'), 'Fix the header')
    await user.click(screen.getByRole('button', { name: 'Add task' }))

    await waitFor(() =>
      expect(calls.some((call) => call.method === 'POST' && call.path === '/api/tasks')).toBe(true),
    )
  })

  it('opens the projects panel from the sidebar, and the settings of a project from it', async () => {
    installDaemon([runningTask])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')

    const sidebar = screen.getByRole('navigation', { name: 'Workspaces' })
    await user.click(within(sidebar).getByRole('button', { name: 'Add a project' }))
    const panel = await screen.findByRole('dialog', { name: 'Projects' })
    await user.click(await within(panel).findByRole('button', { name: 'Settings' }))

    const settings = await screen.findByRole('dialog', { name: 'Project settings of natamux' })
    await waitFor(() =>
      expect(settings.textContent).toContain('Saved in /work/natamux/.natamux.json.'),
    )
  })

  it('names the project of a task in its details', async () => {
    installDaemon([makeTask({ id: 'T-0001', title: 'Fix timezone bug', repo: 'web' })])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')

    await user.click(
      within(screen.getByRole('table')).getByRole('button', { name: /Fix timezone bug/ }),
    )
    const details = screen.getByRole('complementary', { name: 'Details of T-0001' })
    await user.click(within(details).getByRole('button', { name: /^Details/ }))
    expect(within(details).getByText('Project')).toBeTruthy()
    expect(within(details).queryByText('Repository')).toBeNull()
  })

  it('opens Start a task from the sidebar header, the project group and its empty line', async () => {
    installDaemon([makeTask({ id: 'T-0001', title: 'Fix timezone bug', state: 'ready' })])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')
    const sidebar = screen.getByRole('navigation', { name: 'Workspaces' })
    expect(within(sidebar).getByText(/No workspaces\./)).toBeTruthy()

    const openers = [
      ...within(sidebar).getAllByRole('button', { name: 'Start a task' }),
      within(sidebar).getByRole('button', { name: 'Start a task in natamux' }),
    ]
    expect(openers).toHaveLength(3)
    for (const opener of openers) {
      await user.click(opener)
      const dialog = await screen.findByRole('dialog', { name: 'Start a task' })
      expect((within(dialog).getByLabelText('Project') as HTMLSelectElement).value).toBe('default')
      await user.keyboard('{Escape}')
      await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull())
    }
  })

  it('starts a task in the project of the group whose + was clicked', async () => {
    installDaemon([runningTask])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')

    await user.click(screen.getByRole('button', { name: 'Projects' }))
    const panel = await screen.findByRole('dialog', { name: 'Projects' })
    await user.type(within(panel).getByLabelText('Project folder'), '/work/web')
    await user.type(within(panel).getByLabelText('Project name'), 'Web <app> & co')
    await user.click(within(panel).getByRole('button', { name: 'Add project' }))
    expect(await within(panel).findByText('/work/web')).toBeTruthy()
    await user.keyboard('{Escape}')

    const sidebar = screen.getByRole('navigation', { name: 'Workspaces' })
    await user.click(
      await within(sidebar).findByRole('button', { name: 'Start a task in Web <app> & co' }),
    )
    const dialog = await screen.findByRole('dialog', { name: 'Start a task' })
    expect((within(dialog).getByLabelText('Project') as HTMLSelectElement).value).toBe('web')
  })

  it('offers Add task and Start a task when there are no tasks', async () => {
    installDaemon([])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)

    const empty = (await screen.findByText('No tasks yet')).parentElement as HTMLElement
    expect(
      within(empty).getByText('Add a task to the backlog, or start one now in its own workspace.'),
    ).toBeTruthy()
    expect(within(empty).getByRole('button', { name: /^Add task/ })).toBeTruthy()
    await user.click(within(empty).getByRole('button', { name: /^Start a task/ }))
    expect(await screen.findByRole('dialog', { name: 'Start a task' })).toBeTruthy()
  })

  it('starts a task with the typed title from the button next to Add task', async () => {
    installDaemon([runningTask])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')

    await user.type(screen.getByLabelText('New task title'), 'Fix the login')
    const main = screen.getByRole('main')
    await user.click(within(main).getByRole('button', { name: /^Start a task/ }))

    const dialog = await screen.findByRole('dialog', { name: 'Start a task' })
    expect((within(dialog).getByLabelText('Task title') as HTMLInputElement).value).toBe(
      'Fix the login',
    )
  })

  it('removes a project from its sidebar group after asking', async () => {
    const { calls } = installDaemon([runningTask], [], [defaultRepo, webRepo])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')
    const sidebar = screen.getByRole('navigation', { name: 'Workspaces' })
    const removeCalls = () =>
      calls.filter((call) => call.method === 'DELETE' && call.path === '/api/repos/web')

    await user.click(await within(sidebar).findByRole('button', { name: 'Actions for natamux' }))
    expect(screen.getByRole('menuitem', { name: 'Start a task' })).toBeTruthy()
    expect(screen.getByRole('menuitem', { name: 'Settings' })).toBeTruthy()
    expect(screen.getByRole('menuitem', { name: 'Remove project' })).toBeTruthy()
    await user.keyboard('{Escape}')

    await user.click(within(sidebar).getByRole('button', { name: 'Actions for Web app' }))
    await user.click(screen.getByRole('menuitem', { name: 'Remove project' }))
    let confirm = screen.getByRole('dialog', { name: 'Remove Web app from Natamux?' })
    expect(within(confirm).getByText(/\/work\/web/)).toBeTruthy()
    await user.click(within(confirm).getByRole('button', { name: 'Cancel' }))
    expect(removeCalls()).toHaveLength(0)

    await user.click(within(sidebar).getByRole('button', { name: 'Actions for Web app' }))
    await user.click(screen.getByRole('menuitem', { name: 'Remove project' }))
    await user.keyboard('{Escape}')
    expect(screen.queryByRole('dialog', { name: 'Remove Web app from Natamux?' })).toBeNull()
    expect(removeCalls()).toHaveLength(0)

    await user.pointer({
      keys: '[MouseRight]',
      target: within(sidebar).getByRole('button', { name: /^Web app/ }),
    })
    await user.click(screen.getByRole('menuitem', { name: 'Remove project' }))
    confirm = screen.getByRole('dialog', { name: 'Remove Web app from Natamux?' })
    await user.click(within(confirm).getByRole('button', { name: 'Remove' }))

    expect(await screen.findByText('Removed Web app.')).toBeTruthy()
    await waitFor(() =>
      expect(within(sidebar).queryByRole('button', { name: 'Actions for Web app' })).toBeNull(),
    )
    expect(removeCalls()).toHaveLength(1)
  })

  it('keeps the remove item off while the project has workspaces', async () => {
    const { calls } = installDaemon(
      [makeTask({ id: 'T-0003', title: 'Fix header', repo: 'web', workspace: '/work/T-0003' })],
      [],
      [defaultRepo, webRepo],
    )
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')
    const sidebar = screen.getByRole('navigation', { name: 'Workspaces' })

    const header = await within(sidebar).findByRole('button', { name: /^Web app/ })
    act(() => header.focus())
    await user.keyboard('{Shift>}{F10}{/Shift}')

    const remove = await screen.findByRole('menuitem', { name: 'Remove project' })
    expect((remove as HTMLButtonElement).disabled).toBe(true)
    expect(remove.getAttribute('aria-describedby')).toBeTruthy()
    expect(screen.getByText('Remove its workspaces first')).toBeTruthy()
    expect(calls.some((call) => call.path === '/api/repos/web')).toBe(false)
  })

  it('counts a finished workspace as one that keeps the project', async () => {
    installDaemon(
      [
        makeTask({
          id: 'T-0006',
          title: 'Shipped',
          state: 'done',
          repo: 'web',
          workspace: '/work/T-0006',
        }),
      ],
      [],
      [defaultRepo, webRepo],
    )
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')
    const sidebar = screen.getByRole('navigation', { name: 'Workspaces' })

    await user.click(await within(sidebar).findByRole('button', { name: 'Actions for Web app' }))

    const remove = screen.getByRole('menuitem', { name: 'Remove project' })
    expect((remove as HTMLButtonElement).disabled).toBe(true)
  })

  it('removes the finished workspaces of a project after asking, and leaves the rest', async () => {
    const { calls } = installDaemon(
      [
        makeTask({ id: 'T-0006', title: 'Shipped', state: 'done', workspace: '/work/T-0006' }),
        makeTask({ id: 'T-0007', title: 'Dropped', state: 'cancelled', workspace: '/work/T-0007' }),
        makeTask({
          id: 'T-0008',
          title: 'Still going',
          state: 'running',
          workspace: '/work/T-0008',
        }),
      ],
      [],
      [defaultRepo],
    )
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')
    const sidebar = screen.getByRole('navigation', { name: 'Workspaces' })
    const deletes = () =>
      calls.filter((call) => call.method === 'DELETE').map((call) => call.path.split('?')[0])

    await user.click(
      await within(sidebar).findByRole('button', { name: 'Remove finished workspaces in natamux' }),
    )
    const confirm = screen.getByRole('dialog', { name: 'Remove 2 finished workspaces?' })
    expect(within(confirm).getByText('T-0006')).toBeTruthy()
    expect(within(confirm).getByText('T-0007')).toBeTruthy()
    await user.click(within(confirm).getByRole('button', { name: 'Remove' }))

    expect(await screen.findByText('Removed 2 workspaces.')).toBeTruthy()
    // The sidebar order: the newest first, then the higher id.
    expect(deletes()).toEqual(['/api/tasks/T-0007/workspace', '/api/tasks/T-0006/workspace'])
    await waitFor(() => expect(within(sidebar).queryByText('2 finished')).toBeNull())
    expect(within(sidebar).getAllByText('Still going').length).toBeGreaterThan(0)
    expect(useToasts.getState().toasts.map((item) => item.text)).toEqual(['Removed 2 workspaces.'])
  })

  it('removes finished workspaces one by one, skips a failure, and reports once', async () => {
    const { calls } = installDaemon([
      makeTask({
        id: 'T-0006',
        title: 'Shipped',
        state: 'done',
        workspace: '/work/T-0006',
        createdAt: '2026-10-08T12:00:00.000Z',
      }),
      makeTask({
        id: 'T-0007',
        title: 'Dirty',
        state: 'done',
        workspace: '/work/T-0007',
        createdAt: '2026-10-08T11:00:00.000Z',
      }),
      makeTask({
        id: 'T-0008',
        title: 'Dropped',
        state: 'cancelled',
        workspace: '/work/T-0008',
        createdAt: '2026-10-08T10:00:00.000Z',
      }),
    ])
    const dirty = 'T-0007 has 2 changes that are not committed. Commit or discard them first'
    // Each removal waits until the test lets it answer.
    const daemon = globalThis.fetch
    const waiting = new Map<string, () => void>()
    vi.stubGlobal('fetch', async (path: string, init?: RequestInit) => {
      const id = /^\/api\/tasks\/(T-\d+)\/workspace/.exec(path)?.[1]
      if (init?.method === 'DELETE' && id !== undefined) {
        await new Promise<void>((resolve) => waiting.set(id, resolve))
        waiting.delete(id)
        if (id === 'T-0007') {
          return json({ error: 'conflict', message: dirty }, 409)
        }
      }
      return daemon(path, init)
    })
    const answer = async (id: string) => {
      await waitFor(() => expect(waiting.has(id)).toBe(true))
      // Only one removal waits at a time.
      expect([...waiting.keys()]).toEqual([id])
      act(() => waiting.get(id)?.())
    }
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')
    const sidebar = screen.getByRole('navigation', { name: 'Workspaces' })
    const button = () =>
      within(sidebar).getByRole('button', {
        name: 'Remove finished workspaces in natamux',
      }) as HTMLButtonElement

    await user.click(await within(sidebar).findByText('3 finished'))
    await user.click(button())
    await user.click(screen.getByRole('button', { name: 'Remove' }))

    await waitFor(() => expect(button().disabled).toBe(true))
    await answer('T-0006')
    await waitFor(() => expect(within(sidebar).queryByText('Shipped')).toBeNull())
    expect(within(sidebar).getByText('2 finished')).toBeTruthy()
    expect(within(sidebar).getByText('Dropped')).toBeTruthy()

    await answer('T-0007')
    await answer('T-0008')
    await waitFor(() => expect(within(sidebar).queryByText('Dropped')).toBeNull())

    expect(await screen.findByText('Removed 2 of 3 workspaces.')).toBeTruthy()
    expect(screen.getByText(`Not removed: ${dirty}`)).toBeTruthy()
    expect(useToasts.getState().toasts).toHaveLength(1)
    expect(within(sidebar).getByText('1 finished')).toBeTruthy()
    expect(within(sidebar).getByText('Dirty')).toBeTruthy()
    expect(button().disabled).toBe(false)
    expect(calls.filter((call) => call.method === 'DELETE').map((call) => call.path)).toEqual([
      '/api/tasks/T-0006/workspace?deleteBranch=true',
      '/api/tasks/T-0008/workspace?deleteBranch=true',
    ])
  })

  it('forgets a removed project everywhere and keeps its tasks', async () => {
    const { calls } = installDaemon(
      [makeTask({ id: 'T-0005', title: 'Draft for web', repo: 'web' })],
      [],
      [defaultRepo, webRepo],
    )
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')
    const sidebar = screen.getByRole('navigation', { name: 'Workspaces' })
    const repoInPalette = async () => {
      await user.click(
        within(screen.getByRole('banner')).getByRole('button', { name: /Search or run a command/ }),
      )
      const palette = screen.getByRole('dialog', { name: 'Command palette' })
      await user.keyboard('Web app')
      const found = within(palette)
        .queryAllByRole('option')
        .some((option) => option.textContent?.includes('Start a task here'))
      await user.keyboard('{Escape}')
      return found
    }

    expect(await repoInPalette()).toBe(true)
    await user.selectOptions(await screen.findByLabelText('Project'), 'web')

    await user.click(within(sidebar).getByRole('button', { name: 'Actions for Web app' }))
    await user.click(screen.getByRole('menuitem', { name: 'Remove project' }))
    const confirm = screen.getByRole('dialog', { name: 'Remove Web app from Natamux?' })
    await user.click(within(confirm).getByRole('button', { name: 'Remove' }))
    expect(await screen.findByText('Removed Web app.')).toBeTruthy()

    await waitFor(() => expect(screen.queryByLabelText('Project')).toBeNull())
    expect(within(screen.getByRole('table')).getByText('Draft for web')).toBeTruthy()
    expect(await repoInPalette()).toBe(false)

    await user.click(screen.getByRole('button', { name: 'Projects' }))
    const panel = await screen.findByRole('dialog', { name: 'Projects' })
    expect(within(panel).getByText('/work/natamux')).toBeTruthy()
    expect(within(panel).queryByText('/work/web')).toBeNull()
    await user.keyboard('{Escape}')

    await user.type(screen.getByLabelText('New task title'), 'After the remove')
    await user.click(screen.getByRole('button', { name: 'Add task' }))
    await waitFor(() =>
      expect(calls.some((call) => call.method === 'POST' && call.path === '/api/tasks')).toBe(true),
    )
    const created = calls.find((call) => call.method === 'POST' && call.path === '/api/tasks')
    expect(JSON.parse(created?.body ?? '{}').repo).toBe('default')
  })

  it.each([
    ['the default project', defaultRepo],
    ['another project', webRepo],
  ])('removes %s when it is the last one, and then offers to add one', async (_name, last) => {
    const { calls } = installDaemon([], [], [last])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    const sidebar = screen.getByRole('navigation', { name: 'Workspaces' })
    await user.type(screen.getByLabelText('New task title'), 'Later')
    expect(byTip(document.body, 'Add a project first')).toBeNull()

    const actions = `Actions for ${last.name}`
    await user.click(await within(sidebar).findByRole('button', { name: actions }))
    await user.click(screen.getByRole('menuitem', { name: 'Remove project' }))
    const confirm = screen.getByRole('dialog', { name: `Remove ${last.name} from Natamux?` })
    await user.click(within(confirm).getByRole('button', { name: 'Remove' }))

    expect(await within(sidebar).findByText('No projects yet')).toBeTruthy()
    expect(within(sidebar).getByRole('button', { name: 'Add project' })).toBeTruthy()
    expect(byTip(document.body, 'Add a project first')).toBeTruthy()
    expect(
      calls.filter((call) => call.method === 'DELETE' && call.path === `/api/repos/${last.id}`),
    ).toHaveLength(1)
  })

  it('starts with no projects: the sidebar offers one, and new tasks wait for it', async () => {
    const { calls } = installDaemon([], [], [])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    const sidebar = screen.getByRole('navigation', { name: 'Workspaces' })
    expect(await within(sidebar).findByText('No projects yet')).toBeTruthy()

    await user.type(screen.getByLabelText('New task title'), 'Too early{Enter}')
    expect(byTip(document.body, 'Add a project first')).toBeTruthy()
    expect(calls.some((call) => call.method === 'POST' && call.path === '/api/tasks')).toBe(false)

    const titleBar = screen.getByRole('banner')
    await user.click(within(titleBar).getByRole('button', { name: /^Start a task/ }))
    expect(await screen.findByText('Add a project first.')).toBeTruthy()
    expect(screen.queryByRole('dialog', { name: 'Start a task' })).toBeNull()
    const panel = await screen.findByRole('dialog', { name: 'Projects' })
    await user.keyboard('{Escape}')
    await waitFor(() => expect(panel.isConnected).toBe(false))

    await user.keyboard('c')
    expect(screen.queryByRole('dialog', { name: 'Start a task' })).toBeNull()
    const byKey = await screen.findByRole('dialog', { name: 'Projects' })
    await user.keyboard('{Escape}')
    await waitFor(() => expect(byKey.isConnected).toBe(false))

    await user.click(within(sidebar).getByRole('button', { name: 'Add project' }))
    const again = await screen.findByRole('dialog', { name: 'Projects' })
    await user.type(within(again).getByLabelText('Project folder'), '/work/web')
    await user.click(within(again).getByRole('button', { name: 'Add project' }))

    expect(await within(sidebar).findByRole('button', { name: 'Actions for web' })).toBeTruthy()
    expect(within(sidebar).queryByText('No projects yet')).toBeNull()
    await user.keyboard('{Escape}')
    await waitFor(() => expect(byTip(document.body, 'Add a project first')).toBeNull())
    expect(
      calls.filter((call) => call.method === 'POST' && call.path === '/api/repos'),
    ).toHaveLength(1)

    await user.click(within(screen.getByRole('banner')).getByRole('button', { name: /^Start/ }))
    expect(await screen.findByRole('dialog', { name: 'Start a task' })).toBeTruthy()
  })

  it('loads the projects again when the daemon says they changed', async () => {
    const { setRepos } = installDaemon([runningTask], [], [defaultRepo])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')
    const sidebar = screen.getByRole('navigation', { name: 'Workspaces' })
    await within(sidebar).findByRole('button', { name: 'Actions for natamux' })

    setRepos([defaultRepo, webRepo])
    act(() => {
      for (const socket of FakeSocket.instances) {
        socket.onmessage?.({ data: JSON.stringify({ kind: 'repos.changed' }) })
      }
    })

    expect(await within(sidebar).findByRole('button', { name: 'Actions for Web app' })).toBeTruthy()
  })

  it('loads the projects again after the live connection comes back', async () => {
    const { setRepos } = installDaemon([runningTask], [], [defaultRepo])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')
    const sidebar = screen.getByRole('navigation', { name: 'Workspaces' })
    await within(sidebar).findByRole('button', { name: 'Actions for natamux' })
    const socket = FakeSocket.instances[0]
    act(() => socket?.onopen?.())

    // A project added while the socket was down: its repos.changed message never arrives.
    act(() => socket?.onclose?.())
    setRepos([defaultRepo, webRepo])
    act(() => socket?.onopen?.())

    expect(await within(sidebar).findByRole('button', { name: 'Actions for Web app' })).toBeTruthy()
  })

  it('shows why the daemon did not remove a project and keeps its group', async () => {
    const { setTasks } = installDaemon([runningTask], [], [defaultRepo, webRepo])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')
    const sidebar = screen.getByRole('navigation', { name: 'Workspaces' })
    setTasks([makeTask({ id: 'T-0004', repo: 'web', workspace: '/work/T-0004' })])

    await user.click(await within(sidebar).findByRole('button', { name: 'Actions for Web app' }))
    await user.click(screen.getByRole('menuitem', { name: 'Remove project' }))
    const confirm = screen.getByRole('dialog', { name: 'Remove Web app from Natamux?' })
    await user.click(within(confirm).getByRole('button', { name: 'Remove' }))

    expect(await screen.findByText('Remove the 1 workspace of web first')).toBeTruthy()
    expect(within(sidebar).getByRole('button', { name: 'Actions for Web app' })).toBeTruthy()
  })

  it('opens the project settings from the project menu', async () => {
    installDaemon([runningTask], [], [defaultRepo, webRepo])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')
    const sidebar = screen.getByRole('navigation', { name: 'Workspaces' })

    await user.click(await within(sidebar).findByRole('button', { name: 'Actions for Web app' }))
    await user.click(screen.getByRole('menuitem', { name: 'Settings' }))

    const settings = await screen.findByRole('dialog', { name: 'Project settings of Web app' })
    expect(
      await within(settings).findByRole('tab', { name: 'Stages', selected: true }),
    ).toBeTruthy()
    expect(within(settings).getAllByRole('switch')).toHaveLength(8)
  })

  it('compares tries and keeps one, cancelling the others', async () => {
    const group = 'tries-t-0001'
    const { calls } = installDaemon([
      makeTask({ id: 'T-0001', title: 'Speed up', labels: [group], state: 'review' }),
      makeTask({ id: 'T-0002', title: 'Speed up (try 2)', labels: [group], state: 'review' }),
    ])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')

    const table = screen.getByRole('table')
    await user.click(within(table).getByRole('button', { name: /Speed up \(try 2\)/ }))
    const details = screen.getByRole('complementary', { name: 'Details of T-0002' })
    const tries = within(details).getByRole('region', { name: 'Tries' })
    const keep = within(tries).getAllByRole('button', { name: 'Keep this try' })[1] as HTMLElement
    await user.click(keep)
    const confirm = screen.getByRole('dialog', { name: 'Keep T-0002?' })
    await user.click(
      within(confirm).getByRole('button', { name: 'Keep this try, cancel the others' }),
    )

    expect(await screen.findByText('Cancelled 1 other try.')).toBeTruthy()
    expect(
      calls.some((call) => call.method === 'POST' && call.path === '/api/tasks/T-0001/events'),
    ).toBe(true)
  })

  it('shows what the agent of a running task does now', async () => {
    installDaemon([runningTask])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    const table = await screen.findByRole('table')

    expect(await within(table).findByText('Bash: pnpm test')).toBeTruthy()
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

  it('moves the selection with j and opens the workspace with Enter', async () => {
    installDaemon([reviewTask, runningTask])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')

    await user.keyboard('j')
    await user.keyboard('{Enter}')

    expect(screen.getByRole('region', { name: /Workspace of T-000/ })).toBeTruthy()
  })

  it('opens the workspace of a task from the sidebar', async () => {
    installDaemon([
      reviewTask,
      makeTask({
        id: 'T-0002',
        title: 'Add export',
        state: 'running',
        updatedAt: new Date().toISOString(),
        workspace: '/work/T-0002',
        branch: 'natamux/t-0002',
      }),
    ])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')
    const sidebar = screen.getByRole('navigation', { name: 'Workspaces' })

    await user.click(within(sidebar).getByRole('button', { name: /Add export/ }))
    const view = await screen.findByRole('region', { name: 'Changes of T-0002' })
    expect(await within(view).findByText('Terminal stand-in')).toBeTruthy()
    expect(screen.queryByRole('table')).toBeNull()

    await user.click(within(sidebar).getByRole('button', { name: /Fix timezone bug/ }))
    const empty = await screen.findByRole('region', { name: 'Workspace of T-0001' })
    expect(within(empty).getByText(/no workspace yet/)).toBeTruthy()
    // No workspace, no progress to watch: the details stay closed.
    expect(screen.queryByRole('complementary', { name: 'Details of T-0001' })).toBeNull()

    await user.click(within(sidebar).getByRole('button', { name: /All tasks/ }))
    expect(await screen.findByRole('table')).toBeTruthy()
  })

  it('opens the changes from the sidebar with the details beside them', async () => {
    installDaemon([
      makeTask({
        id: 'T-0002',
        title: 'Add export',
        state: 'running',
        updatedAt: new Date().toISOString(),
        workspace: '/work/T-0002',
        branch: 'natamux/t-0002',
      }),
    ])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')
    const sidebar = screen.getByRole('navigation', { name: 'Workspaces' })

    await user.click(within(sidebar).getByRole('button', { name: 'Actions for T-0002' }))
    await user.click(screen.getByRole('menuitem', { name: 'View changes' }))
    expect(await screen.findByRole('region', { name: 'Changes of T-0002' })).toBeTruthy()
    expect(screen.getByRole('complementary', { name: 'Details of T-0002' })).toBeTruthy()

    await user.click(screen.getByRole('button', { name: 'Hide details' }))
    expect(screen.queryByRole('complementary', { name: 'Details of T-0002' })).toBeNull()
    await user.click(screen.getByRole('button', { name: 'Show details' }))
    expect(screen.getByRole('complementary', { name: 'Details of T-0002' })).toBeTruthy()
  })

  it('keeps the details of a task open after a visit to another workspace', async () => {
    installDaemon([
      reviewTask,
      makeTask({
        id: 'T-0002',
        title: 'Add export',
        state: 'running',
        updatedAt: new Date().toISOString(),
        workspace: '/work/T-0002',
        branch: 'natamux/t-0002',
      }),
    ])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')
    const sidebar = screen.getByRole('navigation', { name: 'Workspaces' })

    await user.click(
      within(screen.getByRole('table')).getByRole('button', { name: /Fix timezone bug/ }),
    )
    expect(screen.getByRole('complementary', { name: 'Details of T-0001' })).toBeTruthy()

    await user.click(within(sidebar).getByRole('button', { name: /Add export/ }))
    await screen.findByRole('region', { name: 'Changes of T-0002' })
    expect(screen.getByRole('complementary', { name: 'Details of T-0002' })).toBeTruthy()

    await user.click(within(sidebar).getByRole('button', { name: /Fix timezone bug/ }))
    await screen.findByRole('region', { name: 'Workspace of T-0001' })
    expect(screen.getByRole('complementary', { name: 'Details of T-0001' })).toBeTruthy()
  })

  it('edits a selected task with closed details', async () => {
    installDaemon([makeTask({ id: 'T-0001', title: 'Fix timezone bug' })])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')

    await user.keyboard('j')
    expect(screen.queryByRole('complementary', { name: 'Details of T-0001' })).toBeNull()
    await user.keyboard('e')
    const details = screen.getByRole('complementary', { name: 'Details of T-0001' })
    expect(await within(details).findByRole('form', { name: 'Edit T-0001' })).toBeTruthy()
  })

  it('shows no details on start, even for a selected task', async () => {
    window.localStorage.setItem('natamux.detail-open', '{"T-0001":true}')
    useViewStore.setState({ selectedId: 'T-0001' })
    installDaemon([reviewTask])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')

    expect(screen.queryByRole('complementary', { name: 'Details of T-0001' })).toBeNull()
    expect(screen.getByRole('button', { name: 'Show details' })).toBeTruthy()
  })

  it('keeps the details of another task as they were when a workspace is made', async () => {
    installDaemon([runningTask])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')

    await user.click(within(screen.getByRole('table')).getByRole('button', { name: /Add export/ }))
    expect(screen.getByRole('complementary', { name: 'Details of T-0002' })).toBeTruthy()

    // The details have a header too; the title bar comes first.
    const [titleBar] = screen.getAllByRole('banner')
    await user.click(within(titleBar as HTMLElement).getByRole('button', { name: 'Start a task' }))
    const dialog = screen.getByRole('dialog', { name: 'Start a task' })
    await user.type(within(dialog).getByLabelText('Task title'), 'Fix the login')
    await user.click(within(dialog).getByRole('button', { name: 'Start task' }))

    await screen.findByRole('region', { name: 'Changes of T-0098' })
    expect(screen.getByRole('complementary', { name: 'Details of T-0098' })).toBeTruthy()
    expect(useViewStore.getState().selectedId).toBe('T-0098')
    expect(useViewStore.getState().detailOpenByTask).toEqual({ 'T-0002': true })
  })

  it('opens a subtask from the details and leaves its details as they were', async () => {
    installDaemon([
      makeTask({ id: 'T-0001', title: 'Release 1.0', state: 'review', stage: 'review' }),
      makeTask({ id: 'T-0003', title: 'Write the notes', parentId: 'T-0001' }),
    ])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')
    const sidebar = screen.getByRole('navigation', { name: 'Workspaces' })

    await user.click(within(sidebar).getByRole('button', { name: /Release 1\.0/ }))
    await screen.findByRole('region', { name: 'Workspace of T-0001' })
    await user.click(screen.getByRole('button', { name: 'Show details' }))
    const details = screen.getByRole('complementary', { name: 'Details of T-0001' })
    const subtasks = within(details).getByRole('region', { name: 'Subtasks' })
    await user.click(within(subtasks).getByRole('button', { name: /Write the notes/ }))

    expect(await screen.findByRole('region', { name: 'Workspace of T-0003' })).toBeTruthy()
    expect(screen.queryByRole('complementary', { name: 'Details of T-0003' })).toBeNull()
    expect(useViewStore.getState().detailOpenByTask['T-0001']).toBe(true)
  })

  it('shows and hides the details with Mod+Alt+B', async () => {
    installDaemon([reviewTask])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')

    // No task is selected yet: nothing to show.
    await user.keyboard('{Control>}{Alt>}b{/Alt}{/Control}')
    expect(screen.queryByRole('complementary', { name: 'Details of T-0001' })).toBeNull()

    await user.keyboard('j')
    await user.keyboard('{Control>}{Alt>}b{/Alt}{/Control}')
    expect(screen.getByRole('complementary', { name: 'Details of T-0001' })).toBeTruthy()
    expect(screen.getByRole('button', { name: 'Hide details' })).toBeTruthy()
    await user.keyboard('{Control>}{Alt>}b{/Alt}{/Control}')
    expect(screen.queryByRole('complementary', { name: 'Details of T-0001' })).toBeNull()
    expect(screen.getByRole('button', { name: 'Show details' })).toBeTruthy()
  })

  it('shows and hides the details of the selected task with i', async () => {
    installDaemon([reviewTask])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')

    await user.keyboard('i')
    expect(screen.queryByRole('complementary', { name: 'Details of T-0001' })).toBeNull()

    await user.keyboard('j')
    await user.keyboard('i')
    expect(screen.getByRole('complementary', { name: 'Details of T-0001' })).toBeTruthy()
    await user.keyboard('i')
    expect(screen.queryByRole('complementary', { name: 'Details of T-0001' })).toBeNull()
  })

  it('shows the details of a task address, and none for an unknown task', async () => {
    installDaemon([reviewTask, runningTask])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')

    act(() => useViewStore.getState().applyRoute({ kind: 'task', id: 'T-0002' }))
    expect(screen.getByRole('complementary', { name: 'Details of T-0002' })).toBeTruthy()

    act(() => useViewStore.getState().applyRoute({ kind: 'task', id: 'T-9999' }))
    expect(screen.queryByRole('complementary', { name: /Details of/ })).toBeNull()
    expect(screen.getByRole('table')).toBeTruthy()
  })

  it('closes the details with their close button and with Escape', async () => {
    installDaemon([reviewTask])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')
    const row = () =>
      within(screen.getByRole('table')).getByRole('button', { name: /Fix timezone bug/ })

    await user.click(row())
    const details = screen.getByRole('complementary', { name: 'Details of T-0001' })
    await user.click(within(details).getByRole('button', { name: 'Close details' }))
    expect(screen.queryByRole('complementary', { name: 'Details of T-0001' })).toBeNull()

    await user.click(row())
    await user.click(row())
    expect(screen.getByRole('complementary', { name: 'Details of T-0001' })).toBeTruthy()
    await user.keyboard('{Escape}')
    expect(screen.queryByRole('complementary', { name: 'Details of T-0001' })).toBeNull()
  })

  it('shows no details sheet in a narrow window until asked', async () => {
    vi.stubGlobal('matchMedia', (media: string) => ({
      matches: false,
      media,
      onchange: null,
      addEventListener: () => {},
      removeEventListener: () => {},
      addListener: () => {},
      removeListener: () => {},
      dispatchEvent: () => false,
    }))
    vi.stubGlobal('innerWidth', 900)
    installDaemon([reviewTask])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')

    await user.keyboard('j')
    expect(screen.queryByRole('dialog', { name: 'Details' })).toBeNull()
    expect(screen.queryByRole('complementary', { name: 'Details of T-0001' })).toBeNull()

    vi.stubGlobal('innerWidth', 1400)
    act(() => window.dispatchEvent(new Event('resize')))
    expect(screen.queryByRole('complementary', { name: 'Details of T-0001' })).toBeNull()
    vi.stubGlobal('innerWidth', 900)
    act(() => window.dispatchEvent(new Event('resize')))
    expect(screen.queryByRole('dialog', { name: 'Details' })).toBeNull()

    await user.keyboard('i')
    expect(screen.getByRole('dialog', { name: 'Details' })).toBeTruthy()
  })

  it('leaves the details as they were when the sidebar is hidden and shown', async () => {
    installDaemon([reviewTask])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')

    await user.click(
      within(screen.getByRole('table')).getByRole('button', { name: /Fix timezone bug/ }),
    )
    await user.click(screen.getByRole('button', { name: 'Hide sidebar' }))
    expect(screen.getByRole('complementary', { name: 'Details of T-0001' })).toBeTruthy()
    await user.click(screen.getByRole('button', { name: 'Show sidebar' }))
    expect(screen.getByRole('complementary', { name: 'Details of T-0001' })).toBeTruthy()
  })

  it('shows the running agents side by side and closes a pane', async () => {
    installDaemon([
      reviewTask,
      makeTask({
        id: 'T-0002',
        title: 'Add export',
        state: 'running',
        updatedAt: new Date().toISOString(),
        workspace: '/work/T-0002',
        branch: 'natamux/t-0002',
      }),
    ])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')

    const sidebar = screen.getByRole('navigation', { name: 'Workspaces' })
    await user.click(within(sidebar).getByRole('button', { name: /Agents/ }))
    const grid = screen.getByRole('region', { name: 'Agents' })
    const pane = within(grid).getByRole('region', { name: 'Agent of T-0002' })
    expect(await within(pane).findByText('Terminal stand-in')).toBeTruthy()

    await user.click(within(pane).getByRole('button', { name: 'Close Agent of T-0002' }))
    expect(within(grid).getByText('No agent is working')).toBeTruthy()

    await user.keyboard('{Escape}')
    expect(await screen.findByRole('table')).toBeTruthy()
  })

  it('names the agent in the header of each agent pane', async () => {
    const working = { state: 'running', updatedAt: new Date().toISOString() } as const
    installDaemon([
      makeTask({ ...working, id: 'T-0002', agent: 'claude-code', workspace: '/work/T-0002' }),
      makeTask({ ...working, id: 'T-0003', agent: null, workspace: '/work/T-0003' }),
    ])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')

    const sidebar = screen.getByRole('navigation', { name: 'Workspaces' })
    await user.click(within(sidebar).getByRole('button', { name: /Agents/ }))
    const grid = screen.getByRole('region', { name: 'Agents' })
    const claude = within(grid).getByRole('region', { name: 'Agent of T-0002' })
    expect(within(claude).getByText('Claude Code')).toBeTruthy()
    const none = within(grid).getByRole('region', { name: 'Agent of T-0003' })
    expect(within(none).queryByText('Agent')).toBeNull()
  })

  it('starts a task with notes from the title bar and opens its terminal', async () => {
    const { calls } = installDaemon([runningTask])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')

    await user.click(
      within(screen.getByRole('banner')).getByRole('button', { name: 'Start a task' }),
    )
    const dialog = screen.getByRole('dialog', { name: 'Start a task' })
    await user.type(within(dialog).getByLabelText('Task title'), 'Fix the login')
    await user.type(within(dialog).getByLabelText('Task notes'), 'Keep the old URL')
    await user.click(within(dialog).getByRole('button', { name: 'Start task' }))

    const view = await screen.findByRole('region', { name: 'Changes of T-0098' })
    expect(await within(view).findByText('Terminal stand-in')).toBeTruthy()
    expect(screen.getByRole('complementary', { name: 'Details of T-0098' })).toBeTruthy()
    expect(screen.getByRole('button', { name: 'Hide details' })).toBeTruthy()
    const sidebar = screen.getByRole('navigation', { name: 'Workspaces' })
    expect(within(sidebar).getByRole('button', { name: /Fix the login/ })).toBeTruthy()
    expect(calls.some((call) => call.method === 'POST' && call.path === '/api/workspaces')).toBe(
      true,
    )
  })

  it('pins a workspace to the top of the sidebar', async () => {
    installDaemon([
      makeTask({
        id: 'T-0002',
        title: 'Add export',
        state: 'running',
        updatedAt: new Date().toISOString(),
        workspace: '/work/T-0002',
        branch: 'natamux/t-0002-add-export',
      }),
    ])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')

    const sidebar = screen.getByRole('navigation', { name: 'Workspaces' })
    await user.click(within(sidebar).getByRole('button', { name: 'Actions for T-0002' }))
    await user.click(screen.getByRole('menuitem', { name: 'Pin to the top' }))

    expect(within(sidebar).getByRole('heading', { name: 'Pinned' })).toBeTruthy()
    expect(window.localStorage.getItem('natamux.pinned')).toBe('["T-0002"]')
  })

  it('shows a change that arrives over the live connection', async () => {
    installDaemon([reviewTask, runningTask])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('img', { name: '1 waiting' })

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

    await waitFor(() => expect(screen.getByRole('img', { name: '2 waiting' })).toBeTruthy())
  })
  it('hides and shows the sidebar from the title bar', async () => {
    installDaemon([reviewTask, runningTask])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')

    const toggle = screen.getByRole('button', { name: 'Hide sidebar' })
    expect(screen.getByRole('navigation', { name: 'Workspaces' })).toBeTruthy()
    await user.click(toggle)
    expect(screen.queryByRole('navigation', { name: 'Workspaces' })).toBeNull()
    expect(window.localStorage.getItem('natamux.sidebar-open')).toBe('false')

    await user.click(screen.getByRole('button', { name: 'Show sidebar' }))
    expect(screen.getByRole('navigation', { name: 'Workspaces' })).toBeTruthy()
  })

  it('shows who needs the user in the title bar and the status bar', async () => {
    installDaemon([reviewTask, runningTask])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')

    expect(
      within(screen.getByRole('banner')).getByRole('button', { name: '1 needs you' }),
    ).toBeTruthy()
    expect(
      within(screen.getByRole('contentinfo')).getByRole('button', { name: '1 needs you' }),
    ).toBeTruthy()
    expect(
      within(screen.getByRole('contentinfo')).getByRole('button', { name: '1 running' }),
    ).toBeTruthy()
  })

  it('toggles the details of the selected task from the title bar', async () => {
    installDaemon([reviewTask])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')

    const toggle = screen.getByRole('button', { name: 'Select a task to show its details' })
    expect(toggle.getAttribute('aria-disabled')).toBe('true')

    await user.click(
      within(screen.getByRole('table')).getByRole('button', { name: /Fix timezone bug/ }),
    )
    expect(screen.getByRole('complementary', { name: 'Details of T-0001' })).toBeTruthy()
    await user.click(screen.getByRole('button', { name: 'Hide details' }))
    expect(screen.queryByRole('complementary', { name: 'Details of T-0001' })).toBeNull()
    await user.click(screen.getByRole('button', { name: 'Show details' }))
    expect(screen.getByRole('complementary', { name: 'Details of T-0001' })).toBeTruthy()
  })

  it('has a skip link and the page landmarks', async () => {
    installDaemon([reviewTask])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')

    expect(screen.getByRole('link', { name: 'Skip to main content' }).getAttribute('href')).toBe(
      '#main',
    )
    expect(screen.getByRole('banner')).toBeTruthy()
    expect(screen.getByRole('main')).toBeTruthy()
    expect(screen.getByRole('contentinfo')).toBeTruthy()
  })

  it('moves between the parts with F6', async () => {
    installDaemon([reviewTask])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')

    await user.keyboard('{F6}')
    expect(
      screen.getByRole('navigation', { name: 'Workspaces' }).contains(document.activeElement),
    ).toBe(true)
    await user.keyboard('{F6}')
    expect(screen.getByRole('main').contains(document.activeElement)).toBe(true)
    await user.keyboard('{Shift>}{F6}{/Shift}')
    expect(
      screen.getByRole('navigation', { name: 'Workspaces' }).contains(document.activeElement),
    ).toBe(true)
  })

  it('changes the UI size from the account menu', async () => {
    installDaemon([reviewTask])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')

    await user.click(screen.getByRole('button', { name: 'Account' }))
    await user.click(screen.getByRole('menuitemcheckbox', { name: 'Large' }))
    expect(document.documentElement.style.getPropertyValue('--ui-zoom')).toBe('1.077')
    expect(window.localStorage.getItem('natamux.ui-size')).toBe('large')
  })

  it('asks before it signs out, and keeps the session on Cancel', async () => {
    installDaemon([reviewTask])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')

    await user.click(screen.getByRole('button', { name: 'Account' }))
    await user.click(screen.getByRole('menuitem', { name: 'Sign out' }))
    const dialog = screen.getByRole('dialog', { name: 'Sign out of Natamux?' })
    expect(within(dialog).getByText('Your tasks keep running.')).toBeTruthy()
    await user.click(within(dialog).getByRole('button', { name: 'Cancel' }))
    expect(screen.queryByRole('dialog')).toBeNull()
    expect(screen.getByRole('table')).toBeTruthy()

    await user.click(screen.getByRole('button', { name: 'Account' }))
    await user.click(screen.getByRole('menuitem', { name: 'Sign out' }))
    await user.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Sign out' }))
    expect(await screen.findByLabelText('Daemon token')).toBeTruthy()
  })

  it('opens the shortcut list with the question mark key', async () => {
    installDaemon([reviewTask])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')

    await user.keyboard('?')
    expect(screen.getByRole('dialog', { name: 'Keyboard shortcuts' })).toBeTruthy()
  })

  it('opens the command palette with Ctrl+K, finds a task and opens its workspace', async () => {
    installDaemon([
      makeTask({
        id: 'T-0001',
        title: 'Fix timezone bug',
        state: 'running',
        updatedAt: new Date().toISOString(),
        workspace: '/work/T-0001',
        branch: 'natamux/t-0001',
      }),
    ])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')

    await user.keyboard('{Control>}k{/Control}')
    const palette = screen.getByRole('dialog', { name: 'Command palette' })
    await user.keyboard('timezone')
    expect(within(palette).getAllByRole('option')[0]?.textContent).toContain('T-0001')
    await user.keyboard('{Enter}')

    expect(await screen.findByRole('region', { name: 'Changes of T-0001' })).toBeTruthy()
    expect(screen.queryByRole('dialog', { name: 'Command palette' })).toBeNull()
  })

  it('opens the Terminal tab for a shell picked in the jump palette', async () => {
    installDaemon([workspaceTask()])
    extraShells = ['2']
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')
    // The task was left on its Changes tab.
    act(() => useViewStore.getState().rememberTab('T-0001', 'changes'))

    await user.keyboard('{Control>}j{/Control}')
    const jump = screen.getByRole('dialog', { name: 'Jump to a workspace' })
    await user.click(await within(jump).findByRole('option', { name: /Shell 2/ }))

    const view = await screen.findByRole('region', { name: 'Changes of T-0001' })
    expect(within(view).getByRole('tab', { name: 'Terminal' }).getAttribute('aria-selected')).toBe(
      'true',
    )
    await waitFor(() => expect(useViewStore.getState().shellRequest).toBeNull())
  })

  it('opens the palette from the title bar, and the jump palette from the needs-you pill', async () => {
    installDaemon([reviewTask, runningTask])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')

    await user.click(
      within(screen.getByRole('banner')).getByRole('button', { name: /Search or run a command/ }),
    )
    expect(screen.getByRole('dialog', { name: 'Command palette' })).toBeTruthy()
    await user.keyboard('{Escape}')
    expect(screen.queryByRole('dialog', { name: 'Command palette' })).toBeNull()

    await user.click(
      within(screen.getByRole('banner')).getByRole('button', { name: '1 needs you' }),
    )
    const jump = screen.getByRole('dialog', { name: 'Jump to a workspace' })
    expect(within(jump).getByRole('button', { name: 'Remove the Needs you scope' })).toBeTruthy()
  })

  it('runs a command from the palette', async () => {
    installDaemon([reviewTask])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')

    await user.keyboard('{Control>}k{/Control}')
    await user.keyboard('>board{Enter}')
    expect(await screen.findByRole('radio', { name: 'Board', checked: true })).toBeTruthy()
  })

  it('leaves single keys to a field and to open layers', async () => {
    installDaemon([reviewTask])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')

    await user.click(screen.getByLabelText('New task title'))
    await user.keyboard('c?n')
    expect((screen.getByLabelText('New task title') as HTMLInputElement).value).toBe('c?n')
    expect(screen.queryByRole('dialog')).toBeNull()

    await user.keyboard('{Escape}')
    await user.keyboard('c')
    expect(screen.getByRole('dialog', { name: 'Start a task' })).toBeTruthy()
    await user.keyboard('{Escape}')
    await user.keyboard('?')
    expect(screen.getByRole('dialog', { name: 'Keyboard shortcuts' })).toBeTruthy()
  })

  it('lists the keys of the registry in the shortcut sheet and filters them', async () => {
    installDaemon([reviewTask])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')

    await user.keyboard('?')
    const sheet = screen.getByRole('dialog', { name: 'Keyboard shortcuts' })
    expect(within(sheet).getByText('Open the command palette')).toBeTruthy()
    expect(within(sheet).getByText('Jump to a workspace')).toBeTruthy()
    expect(within(sheet).getByText('Maximize or restore the focused pane')).toBeTruthy()
    await user.keyboard('jump')
    expect(within(sheet).queryByText('Open the command palette')).toBeNull()
    expect(within(sheet).getByText('Jump to a workspace')).toBeTruthy()
  })

  it('switches the workspace tab with the number keys', async () => {
    installDaemon([
      makeTask({
        id: 'T-0001',
        title: 'Fix timezone bug',
        state: 'running',
        updatedAt: new Date().toISOString(),
        workspace: '/work/T-0001',
        branch: 'natamux/t-0001',
      }),
    ])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')
    useViewStore.getState().openChanges('T-0001', 'terminal')

    const view = await screen.findByRole('region', { name: 'Changes of T-0001' })
    await user.click(within(view).getByRole('tab', { name: 'Terminal' }))
    await user.keyboard('3')
    expect(within(view).getByRole('tab', { name: 'History' }).getAttribute('aria-selected')).toBe(
      'true',
    )
    await user.keyboard('2')
    expect(within(view).getByRole('tab', { name: 'Changes' }).getAttribute('aria-selected')).toBe(
      'true',
    )
    await user.keyboard('4')
    expect(within(view).getByRole('tab', { name: 'Docs' }).getAttribute('aria-selected')).toBe(
      'true',
    )
    expect(within(view).queryByRole('tab', { name: 'Progress' })).toBeNull()
  })

  it('shows the progress in the details beside the terminal, and approves a stage there', async () => {
    const { calls } = installDaemon([
      makeTask({
        id: 'T-0001',
        title: 'Fix timezone bug',
        state: 'review',
        stage: 'review',
        updatedAt: new Date().toISOString(),
        workspace: '/work/T-0001',
        branch: 'natamux/t-0001',
      }),
    ])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')
    act(() => useViewStore.getState().openChanges('T-0001', 'terminal'))

    const view = await screen.findByRole('region', { name: 'Changes of T-0001' })
    const details = screen.getByRole('complementary', { name: 'Details of T-0001' })
    const progressTab = within(details).getByRole('tab', { name: 'Progress' })
    expect(progressTab.getAttribute('aria-selected')).toBe('true')

    const review = await within(details).findByRole('article', { name: 'Review' })
    await user.click(within(review).getByRole('button', { name: 'Approve' }))
    await waitFor(() =>
      expect(
        calls.some((call) => call.method === 'POST' && call.path === '/api/tasks/T-0001/stage'),
      ).toBe(true),
    )
    expect(within(view).getByText('Terminal stand-in')).toBeTruthy()
    expect(within(view).getByRole('tab', { name: 'Terminal' }).getAttribute('aria-selected')).toBe(
      'true',
    )

    await user.click(within(details).getByRole('tab', { name: 'Details' }))
    expect(within(details).getByText('No notes. Add some with Edit.')).toBeTruthy()
    expect(window.localStorage.getItem('natamux.details-view')).toBe('details')
  })

  it('opens the details beside a workspace on Progress, whichever way they open', async () => {
    installDaemon([workspaceTask()])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')
    openWorkspace('T-0001')
    await screen.findByRole('region', { name: 'Changes of T-0001' })
    const progressShown = () => {
      const details = screen.getByRole('complementary', { name: 'Details of T-0001' })
      return within(details).getByRole('tab', { name: 'Progress' }).getAttribute('aria-selected')
    }

    expect(progressShown()).toBe('true')
    await user.click(screen.getByRole('button', { name: 'Hide details' }))
    await user.click(screen.getByRole('button', { name: 'Show details' }))
    expect(progressShown()).toBe('true')
    await user.keyboard('i')
    await user.keyboard('i')
    expect(progressShown()).toBe('true')
    await user.keyboard('i')
    await user.keyboard('{Control>}{Alt>}b{/Alt}{/Control}')
    expect(progressShown()).toBe('true')
  })

  it('keeps the view picked in the details for the next task', async () => {
    installDaemon([workspaceTask(), workspaceTask({ id: 'T-0002', title: 'Add export' })])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')
    openWorkspace('T-0001')
    await screen.findByRole('region', { name: 'Changes of T-0001' })
    const first = screen.getByRole('complementary', { name: 'Details of T-0001' })
    await user.click(within(first).getByRole('tab', { name: 'Details' }))

    openWorkspace('T-0002')
    const second = await screen.findByRole('complementary', { name: 'Details of T-0002' })
    const detailsTab = within(second).getByRole('tab', { name: 'Details' })
    expect(detailsTab.getAttribute('aria-selected')).toBe('true')
    expect(window.localStorage.getItem('natamux.details-view')).toBe('details')
  })

  it('opens a stage file from the progress in the Docs tab, and the progress stays', async () => {
    stageFiles = [{ name: 'spec.md', size: 10, modifiedAt: '2026-10-08T09:30:00.000Z' }]
    installDaemon([workspaceTask({ stage: 'spec', stages: ['spec', 'plan'] })])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')
    openWorkspace('T-0001')
    const view = await screen.findByRole('region', { name: 'Changes of T-0001' })
    const details = screen.getByRole('complementary', { name: 'Details of T-0001' })

    await openStageFile(user, details)

    expect(await within(view).findByRole('heading', { name: 'The spec' })).toBeTruthy()
    const docsTab = within(view).getByRole('tab', { name: 'Docs' })
    expect(docsTab.getAttribute('aria-selected')).toBe('true')
    const progressTab = within(details).getByRole('tab', { name: 'Progress' })
    expect(progressTab.getAttribute('aria-selected')).toBe('true')
  })

  it('opens the workspace on Docs for a stage file picked beside the task list', async () => {
    stageFiles = [{ name: 'spec.md', size: 10, modifiedAt: '2026-10-08T09:30:00.000Z' }]
    installDaemon([workspaceTask({ stage: 'spec', stages: ['spec', 'plan'] })])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')
    await user.click(
      within(screen.getByRole('table')).getByRole('button', { name: /Fix timezone bug/ }),
    )
    const details = screen.getByRole('complementary', { name: 'Details of T-0001' })
    await user.click(within(details).getByRole('tab', { name: 'Progress' }))

    await openStageFile(user, details)

    const view = await screen.findByRole('region', { name: 'Changes of T-0001' })
    expect(await within(view).findByRole('heading', { name: 'The spec' })).toBeTruthy()
    expect(screen.getByRole('complementary', { name: 'Details of T-0001' })).toBeTruthy()
  })

  it('opens a task of the progress layout on its terminal, with the progress beside it', async () => {
    installDaemon([workspaceTask()])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')

    await user.click(screen.getByRole('radio', { name: 'Progress' }))
    const main = screen.getByRole('main')
    await user.click(await within(main).findByRole('button', { name: 'Fix timezone bug' }))

    const view = await screen.findByRole('region', { name: 'Changes of T-0001' })
    const terminalTab = within(view).getByRole('tab', { name: 'Terminal' })
    expect(terminalTab.getAttribute('aria-selected')).toBe('true')
    const details = screen.getByRole('complementary', { name: 'Details of T-0001' })
    const progressTab = within(details).getByRole('tab', { name: 'Progress' })
    expect(progressTab.getAttribute('aria-selected')).toBe('true')
  })

  it('closes the docked details for the Changes tab only', async () => {
    vi.stubGlobal('matchMedia', (media: string) => ({
      matches: false,
      media,
      onchange: null,
      addEventListener: () => {},
      removeEventListener: () => {},
      addListener: () => {},
      removeListener: () => {},
      dispatchEvent: () => false,
    }))
    vi.stubGlobal('innerWidth', 1400)
    installDaemon([workspaceTask()])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')
    openWorkspace('T-0001')
    const view = await screen.findByRole('region', { name: 'Changes of T-0001' })
    const shown = () => screen.queryByRole('complementary', { name: 'Details of T-0001' })
    expect(shown()).toBeTruthy()

    for (const tab of ['History', 'Docs', 'Terminal']) {
      await user.click(within(view).getByRole('tab', { name: tab }))
      expect(shown()).toBeTruthy()
    }
    await user.click(within(view).getByRole('tab', { name: 'Changes' }))
    expect(shown()).toBeNull()
    // The close counts for the task, so the details stay closed on the other tabs.
    await user.click(within(view).getByRole('tab', { name: 'Terminal' }))
    expect(shown()).toBeNull()
  })

  it('opens the details beside a workspace until the user closes them', async () => {
    installDaemon([workspaceTask(), workspaceTask({ id: 'T-0002', title: 'Add export' })])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')
    const shown = (id: string) => screen.queryByRole('complementary', { name: `Details of ${id}` })

    openWorkspace('T-0001')
    await screen.findByRole('region', { name: 'Changes of T-0001' })
    expect(shown('T-0001')).toBeTruthy()
    await user.click(screen.getByRole('button', { name: 'Hide details' }))
    expect(shown('T-0001')).toBeNull()

    openWorkspace('T-0002')
    await screen.findByRole('region', { name: 'Changes of T-0002' })
    expect(shown('T-0002')).toBeTruthy()

    openWorkspace('T-0001')
    await screen.findByRole('region', { name: 'Changes of T-0001' })
    expect(shown('T-0001')).toBeNull()
    await user.click(screen.getByRole('button', { name: 'Show details' }))
    expect(shown('T-0001')).toBeTruthy()

    // A list address hides the details, but that is not the user closing them.
    act(() => useViewStore.getState().applyRoute({ kind: 'tasks' }))
    await screen.findByRole('table')
    expect(shown('T-0001')).toBeNull()
    act(() =>
      useViewStore
        .getState()
        .applyRoute({ kind: 'workspace', id: 'T-0001', tab: 'terminal', file: null }),
    )
    await screen.findByRole('region', { name: 'Changes of T-0001' })
    expect(shown('T-0001')).toBeTruthy()
  })

  it('opens the details of a workspace address on its own', async () => {
    installDaemon([workspaceTask()])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')

    act(() =>
      useViewStore
        .getState()
        .applyRoute({ kind: 'workspace', id: 'T-0001', tab: 'terminal', file: null }),
    )
    await screen.findByRole('region', { name: 'Changes of T-0001' })
    const details = screen.getByRole('complementary', { name: 'Details of T-0001' })
    const progressTab = within(details).getByRole('tab', { name: 'Progress' })
    expect(progressTab.getAttribute('aria-selected')).toBe('true')
  })

  it('does not open the details of a workspace on their own where they would cover it', async () => {
    vi.stubGlobal('matchMedia', (media: string) => ({
      matches: false,
      media,
      onchange: null,
      addEventListener: () => {},
      removeEventListener: () => {},
      addListener: () => {},
      removeListener: () => {},
      dispatchEvent: () => false,
    }))
    vi.stubGlobal('innerWidth', 900)
    installDaemon([workspaceTask()])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')

    openWorkspace('T-0001')
    await screen.findByRole('region', { name: 'Changes of T-0001' })
    expect(screen.queryByRole('dialog', { name: 'Details' })).toBeNull()
    expect(screen.queryByRole('complementary', { name: 'Details of T-0001' })).toBeNull()

    await user.keyboard('{Control>}{Alt>}b{/Alt}{/Control}')
    expect(screen.getByRole('dialog', { name: 'Details' })).toBeTruthy()
  })

  it.each(['table', 'board', 'progress'] as const)(
    'keeps the details closed while the selection moves in the %s layout',
    async (layout) => {
      useViewStore.setState({ layout })
      installDaemon([workspaceTask(), workspaceTask({ id: 'T-0002', title: 'Add export' })])
      const user = userEvent.setup()
      render(<App />)
      await signIn(user)
      await screen.findAllByRole('button', { name: /Fix timezone bug/ })

      await user.keyboard('j')
      await user.keyboard('j')
      await user.keyboard('k')
      expect(useViewStore.getState().selectedId).not.toBeNull()
      expect(screen.queryByRole('complementary', { name: /Details of/ })).toBeNull()
      expect(routeOf(useViewStore.getState()).kind).toBe(layout === 'table' ? 'tasks' : layout)
    },
  )

  it.each([
    [
      'the jump palette',
      async (user: ReturnType<typeof userEvent.setup>) => {
        await user.keyboard('{Control>}k{/Control}')
        await user.keyboard('timezone{Enter}')
      },
    ],
    [
      'Enter on a table row',
      async (user: ReturnType<typeof userEvent.setup>) => {
        await user.keyboard('j')
        await user.keyboard('{Enter}')
      },
    ],
    [
      'the needs-you strip',
      async (user: ReturnType<typeof userEvent.setup>) => {
        const strip = screen.getByRole('region', { name: 'Needs you' })
        const [row] = within(strip).getAllByRole('button', { name: /Fix timezone bug/ })
        await user.click(row as HTMLElement)
      },
    ],
  ])('opens the details beside a workspace opened from %s', async (_way, open) => {
    installDaemon([workspaceTask()])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')

    await open(user)
    await screen.findByRole('region', { name: 'Changes of T-0001' })
    const details = screen.getByRole('complementary', { name: 'Details of T-0001' })
    const progressTab = within(details).getByRole('tab', { name: 'Progress' })
    expect(progressTab.getAttribute('aria-selected')).toBe('true')
    expect(screen.getByRole('button', { name: 'Hide details' })).toBeTruthy()
    // The default is not a choice of the user, so nothing is remembered for the task.
    expect(useViewStore.getState().detailOpenByTask).toEqual({})
  })

  it.each([
    [1280, true],
    [1279, false],
  ])('at a window %i px wide opens the details on its own: %s', async (width, opens) => {
    stubWindowWidth(width)
    installDaemon([workspaceTask()])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)

    openWorkspace('T-0001')
    await screen.findByRole('region', { name: 'Changes of T-0001' })
    const details = screen.queryByRole('complementary', { name: 'Details of T-0001' })
    expect(details !== null).toBe(opens)
    expect(screen.queryByRole('dialog', { name: 'Details' })).toBeNull()
  })

  it('opens the details on the view the user picked last', async () => {
    useViewStore.setState({ detailsView: 'details' })
    installDaemon([workspaceTask(), workspaceTask({ id: 'T-0002', title: 'Add export' })])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')
    const selected = (id: string, tab: string) => {
      const details = screen.getByRole('complementary', { name: `Details of ${id}` })
      return within(details).getByRole('tab', { name: tab }).getAttribute('aria-selected')
    }

    openWorkspace('T-0001')
    await screen.findByRole('region', { name: 'Changes of T-0001' })
    expect(selected('T-0001', 'Details')).toBe('true')

    const first = screen.getByRole('complementary', { name: 'Details of T-0001' })
    await user.click(within(first).getByRole('tab', { name: 'Progress' }))
    openWorkspace('T-0002')
    await screen.findByRole('region', { name: 'Changes of T-0002' })
    expect(selected('T-0002', 'Progress')).toBe('true')
    expect(window.localStorage.getItem('natamux.details-view')).toBe('progress')
  })

  it('opens the details of a Changes address, and shows nothing for an unknown task', async () => {
    installDaemon([workspaceTask()])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')

    act(() =>
      useViewStore
        .getState()
        .applyRoute({ kind: 'workspace', id: 'T-0001', tab: 'changes', file: null }),
    )
    await screen.findByRole('region', { name: 'Changes of T-0001' })
    expect(screen.getByRole('complementary', { name: 'Details of T-0001' })).toBeTruthy()

    act(() =>
      useViewStore
        .getState()
        .applyRoute({ kind: 'workspace', id: 'T-9999', tab: 'terminal', file: null }),
    )
    expect(screen.queryByRole('complementary', { name: /Details of/ })).toBeNull()
  })

  it.each([
    [
      'the close button',
      async (user: ReturnType<typeof userEvent.setup>) => {
        const details = screen.getByRole('complementary', { name: 'Details of T-0001' })
        await user.click(within(details).getByRole('button', { name: 'Close details' }))
      },
    ],
    [
      'the title bar toggle',
      async (user: ReturnType<typeof userEvent.setup>) => {
        await user.click(screen.getByRole('button', { name: 'Hide details' }))
      },
    ],
    [
      'Mod+Alt+B',
      async (user: ReturnType<typeof userEvent.setup>) => {
        await user.keyboard('{Control>}{Alt>}b{/Alt}{/Control}')
      },
    ],
    [
      'i',
      async (user: ReturnType<typeof userEvent.setup>) => {
        await user.keyboard('i')
      },
    ],
  ])('keeps the details of a task closed with %s', async (_control, close) => {
    installDaemon([workspaceTask(), workspaceTask({ id: 'T-0002', title: 'Add export' })])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')
    const shown = (id: string) => screen.queryByRole('complementary', { name: `Details of ${id}` })

    openWorkspace('T-0001')
    await screen.findByRole('region', { name: 'Changes of T-0001' })
    await close(user)
    expect(shown('T-0001')).toBeNull()

    openWorkspace('T-0002')
    await screen.findByRole('region', { name: 'Changes of T-0002' })
    expect(shown('T-0002')).toBeTruthy()

    // Back by another way, after a visit to the list.
    act(() => useViewStore.getState().applyRoute({ kind: 'tasks' }))
    await screen.findByRole('table')
    await user.keyboard('{Control>}k{/Control}')
    await user.keyboard('timezone{Enter}')
    await screen.findByRole('region', { name: 'Changes of T-0001' })
    expect(shown('T-0001')).toBeNull()
  })

  it('keeps the details open that the user opened again, and the last choice wins', async () => {
    installDaemon([workspaceTask(), workspaceTask({ id: 'T-0002', title: 'Add export' })])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')
    const shown = () => screen.queryByRole('complementary', { name: 'Details of T-0001' })

    openWorkspace('T-0001')
    await screen.findByRole('region', { name: 'Changes of T-0001' })
    await user.keyboard('i')
    await user.keyboard('i')
    openWorkspace('T-0002')
    openWorkspace('T-0001')
    await screen.findByRole('region', { name: 'Changes of T-0001' })
    expect(shown()).toBeTruthy()

    await user.keyboard('i')
    await user.keyboard('i')
    await user.keyboard('i')
    openWorkspace('T-0002')
    openWorkspace('T-0001')
    await screen.findByRole('region', { name: 'Changes of T-0001' })
    expect(shown()).toBeNull()
  })

  it('shows the progress from the palette in details that are open already', async () => {
    installDaemon([workspaceTask()])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')

    openWorkspace('T-0001')
    await screen.findByRole('region', { name: 'Changes of T-0001' })
    const details = screen.getByRole('complementary', { name: 'Details of T-0001' })
    await user.click(within(details).getByRole('tab', { name: 'Details' }))

    await user.keyboard('{Control>}k{/Control}')
    await user.keyboard('>Show the task progress{Enter}')
    const progressTab = within(
      screen.getByRole('complementary', { name: 'Details of T-0001' }),
    ).getByRole('tab', { name: 'Progress' })
    expect(progressTab.getAttribute('aria-selected')).toBe('true')
  })

  it.each([{ kind: 'tasks' }, { kind: 'board' }, { kind: 'progress' }] as const)(
    'opens the details again after a visit to the $kind address',
    async (route) => {
      installDaemon([workspaceTask(), workspaceTask({ id: 'T-0002', title: 'Add export' })])
      const user = userEvent.setup()
      render(<App />)
      await signIn(user)
      await screen.findByRole('table')
      const shown = (id: string) =>
        screen.queryByRole('complementary', { name: `Details of ${id}` })
      const visit = async (id: string) => {
        act(() => useViewStore.getState().applyRoute(route))
        await waitFor(() => expect(screen.queryByRole('region', { name: /Changes of/ })).toBeNull())
        expect(shown(id)).toBeNull()
        act(() =>
          useViewStore
            .getState()
            .applyRoute({ kind: 'workspace', id, tab: 'terminal', file: null }),
        )
        await screen.findByRole('region', { name: `Changes of ${id}` })
      }

      openWorkspace('T-0001')
      await screen.findByRole('region', { name: 'Changes of T-0001' })
      await visit('T-0001')
      expect(shown('T-0001')).toBeTruthy()

      // A close of the user stays.
      openWorkspace('T-0002')
      await screen.findByRole('region', { name: 'Changes of T-0002' })
      await user.click(screen.getByRole('button', { name: 'Hide details' }))
      await visit('T-0002')
      expect(shown('T-0002')).toBeNull()
    },
  )

  it('takes the details off a workspace when the window gets narrow, and back when wide', async () => {
    stubWindowWidth(1400)
    installDaemon([workspaceTask()])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)

    openWorkspace('T-0001')
    await screen.findByRole('region', { name: 'Changes of T-0001' })
    expect(screen.getByRole('complementary', { name: 'Details of T-0001' })).toBeTruthy()

    vi.stubGlobal('innerWidth', 900)
    act(() => window.dispatchEvent(new Event('resize')))
    expect(screen.queryByRole('dialog', { name: 'Details' })).toBeNull()
    expect(screen.queryByRole('complementary', { name: 'Details of T-0001' })).toBeNull()

    vi.stubGlobal('innerWidth', 1400)
    act(() => window.dispatchEvent(new Event('resize')))
    expect(screen.getByRole('complementary', { name: 'Details of T-0001' })).toBeTruthy()

    vi.stubGlobal('innerWidth', 900)
    act(() => window.dispatchEvent(new Event('resize')))
    await user.keyboard('i')
    expect(screen.getByRole('dialog', { name: 'Details' })).toBeTruthy()
  })

  it('does not open the details on their own when a wide sidebar leaves main too narrow', async () => {
    window.localStorage.setItem('natamux.sidebar-width', '440')
    stubWindowWidth(1300)
    installDaemon([workspaceTask()])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)

    openWorkspace('T-0001')
    await screen.findByRole('region', { name: 'Changes of T-0001' })
    expect(screen.queryByRole('dialog', { name: 'Details' })).toBeNull()
    expect(screen.queryByRole('complementary', { name: 'Details of T-0001' })).toBeNull()
  })

  it('opens the details once a workspace is made from the empty workspace view', async () => {
    installDaemon([reviewTask])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')

    openWorkspace('T-0001')
    const empty = await screen.findByRole('region', { name: 'Workspace of T-0001' })
    expect(screen.queryByRole('complementary', { name: 'Details of T-0001' })).toBeNull()
    await user.click(within(empty).getByRole('button', { name: 'Create workspace' }))

    await screen.findByRole('region', { name: 'Changes of T-0001' })
    expect(await screen.findByRole('complementary', { name: 'Details of T-0001' })).toBeTruthy()
  })

  it('opens the details on Progress for the old Progress address, also after a close', async () => {
    useViewStore.setState({ detailsView: 'details' })
    installDaemon([workspaceTask()])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')

    openWorkspace('T-0001')
    await screen.findByRole('region', { name: 'Changes of T-0001' })
    await user.click(screen.getByRole('button', { name: 'Hide details' }))
    act(() =>
      useViewStore.getState().applyRoute({
        kind: 'workspace',
        id: 'T-0001',
        tab: 'terminal',
        file: null,
        showProgress: true,
      }),
    )
    const details = await screen.findByRole('complementary', { name: 'Details of T-0001' })
    const progressTab = within(details).getByRole('tab', { name: 'Progress' })
    expect(progressTab.getAttribute('aria-selected')).toBe('true')
  })

  it('keeps the Changes close for its task only, also after a visit to the list', async () => {
    stubWindowWidth(1400)
    installDaemon([workspaceTask(), workspaceTask({ id: 'T-0002', title: 'Add export' })])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    const shown = (id: string) => screen.queryByRole('complementary', { name: `Details of ${id}` })

    openWorkspace('T-0001')
    const view = await screen.findByRole('region', { name: 'Changes of T-0001' })
    await user.click(within(view).getByRole('tab', { name: 'Changes' }))
    expect(shown('T-0001')).toBeNull()

    openWorkspace('T-0002')
    await screen.findByRole('region', { name: 'Changes of T-0002' })
    expect(shown('T-0002')).toBeTruthy()

    act(() => useViewStore.getState().applyRoute({ kind: 'tasks' }))
    openWorkspace('T-0001')
    await screen.findByRole('region', { name: 'Changes of T-0001' })
    expect(shown('T-0001')).toBeNull()

    // Opened again by hand on Changes, they stay.
    await user.click(screen.getByRole('button', { name: 'Show details' }))
    expect(shown('T-0001')).toBeTruthy()
  })

  it('sorts the table from the column headers and says which column is sorted', async () => {
    installDaemon([
      makeTask({ id: 'T-0001', title: 'Alpha' }),
      makeTask({ id: 'T-0002', title: 'Beta' }),
    ])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    const table = await screen.findByRole('table')
    const titles = () =>
      [...table.querySelectorAll('tbody [data-roving]')].map((row) => row.textContent)

    expect(titles()).toEqual(['Beta', 'Alpha'])
    await user.click(within(table).getByRole('button', { name: 'Task' }))
    expect(titles()).toEqual(['Alpha', 'Beta'])
    expect(
      within(table).getByRole('columnheader', { name: 'Task' }).getAttribute('aria-sort'),
    ).toBe('ascending')
    await user.click(within(table).getByRole('button', { name: 'Task' }))
    expect(titles()).toEqual(['Beta', 'Alpha'])
    expect(
      within(table).getByRole('columnheader', { name: 'Task' }).getAttribute('aria-sort'),
    ).toBe('descending')
  })

  it('says when a filter matches nothing, and clears it', async () => {
    installDaemon([
      makeTask({
        id: 'T-0002',
        title: 'Add export',
        state: 'running',
        updatedAt: new Date().toISOString(),
        workspace: '/work/T-0002',
        branch: 'natamux/t-0002',
      }),
    ])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')

    await user.type(screen.getByLabelText('Search tasks'), 'zzzz')
    expect(await screen.findByText('No tasks match “zzzz”')).toBeTruthy()
    expect(screen.getByText('No matches in natamux (1 hidden)')).toBeTruthy()
    expect(screen.getByText('Not filtered')).toBeTruthy()

    await user.click(screen.getByRole('button', { name: 'Clear filter' }))
    expect(await screen.findByRole('table')).toBeTruthy()
    expect((screen.getByLabelText('Search tasks') as HTMLInputElement).value).toBe('')
  })

  it('filters by state from the chips, and counts follow the search', async () => {
    installDaemon([
      makeTask({ id: 'T-0001', title: 'Fix login', state: 'review' }),
      makeTask({ id: 'T-0002', title: 'Fix export', state: 'ready' }),
      makeTask({ id: 'T-0003', title: 'Other thing', state: 'ready' }),
    ])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    const table = await screen.findByRole('table')

    const filter = screen.getByRole('list', { name: 'Filter by state' })
    expect(within(filter).getByRole('button', { name: /Ready/ }).textContent).toContain('2')
    await user.type(screen.getByLabelText('Search tasks'), 'fix')
    expect(within(filter).getByRole('button', { name: /Ready/ }).textContent).toContain('1')

    await user.click(within(filter).getByRole('button', { name: /Ready/ }))
    expect(within(table).queryByText('Fix login')).toBeNull()
    expect(within(table).getByText('Fix export')).toBeTruthy()
  })

  it('hides finished columns on the board until asked', async () => {
    installDaemon([reviewTask, makeTask({ id: 'T-0005', title: 'Shipped', state: 'done' })])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')
    await user.click(screen.getByRole('radio', { name: 'Board' }))

    expect(screen.queryByRole('region', { name: 'Done' })).toBeNull()
    await user.click(screen.getByRole('checkbox', { name: 'Show finished' }))
    const done = screen.getByRole('region', { name: 'Done' })
    expect(within(done).getByText('Shipped')).toBeTruthy()
  })

  it('moves through the table rows with the arrow keys and opens with Enter', async () => {
    installDaemon([
      makeTask({ id: 'T-0001', title: 'Alpha' }),
      makeTask({ id: 'T-0002', title: 'Beta' }),
    ])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    const table = await screen.findByRole('table')
    const rows = () => [...table.querySelectorAll<HTMLElement>('tbody [data-roving]')]

    expect(rows().filter((row) => row.tabIndex === 0)).toHaveLength(1)
    act(() => rows()[0]?.focus())
    await user.keyboard('{ArrowDown}')
    expect(document.activeElement).toBe(rows()[1])
    expect(rows().filter((row) => row.tabIndex === 0)).toEqual([rows()[1]])

    await user.keyboard('i')
    expect(screen.getByRole('complementary', { name: 'Details of T-0001' })).toBeTruthy()
    await user.keyboard('{Enter}')
    expect(await screen.findByRole('region', { name: 'Workspace of T-0001' })).toBeTruthy()
  })

  it('keeps the sidebar tree as one tab stop and renames a row with F2', async () => {
    const { calls } = installDaemon([
      makeTask({
        id: 'T-0002',
        title: 'Add export',
        state: 'running',
        updatedAt: new Date().toISOString(),
        workspace: '/work/T-0002',
        branch: 'natamux/t-0002',
      }),
      makeTask({
        id: 'T-0003',
        title: 'Fix login',
        state: 'running',
        updatedAt: new Date().toISOString(),
        workspace: '/work/T-0003',
        branch: 'natamux/t-0003',
      }),
    ])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')
    const tree = screen.getByRole('region', { name: 'Workspaces' })
    const stops = () => [...tree.querySelectorAll<HTMLElement>('[data-roving]')]

    expect(stops().filter((stop) => stop.tabIndex === 0)).toHaveLength(1)
    act(() => stops()[0]?.focus())
    await user.keyboard('{ArrowDown}')
    expect(document.activeElement).toBe(stops()[1])
    await user.keyboard('{End}')
    expect(document.activeElement).toBe(stops()[stops().length - 1])

    const row = tree.querySelector<HTMLElement>('[data-roving="task:T-0002"]')
    act(() => row?.focus())
    await user.keyboard('{F2}')
    const field = screen.getByLabelText('New name for T-0002') as HTMLInputElement
    expect(field.selectionStart).toBe(0)
    expect(field.selectionEnd).toBe('Add export'.length)
    await user.keyboard('Add CSV export{Enter}')

    await waitFor(() => expect(calls.some((call) => call.method === 'PATCH')).toBe(true))
    expect(document.activeElement?.getAttribute('data-roving')).toBe('task:T-0002')
  })

  it('folds and unfolds a repository with Left and Right', async () => {
    installDaemon([
      makeTask({
        id: 'T-0002',
        title: 'Add export',
        state: 'running',
        updatedAt: new Date().toISOString(),
        workspace: '/work/T-0002',
        branch: 'natamux/t-0002',
      }),
    ])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')
    const tree = screen.getByRole('region', { name: 'Workspaces' })
    const group = tree.querySelector<HTMLElement>('[data-roving="repo:default"]')
    act(() => group?.focus())

    await user.keyboard('{ArrowLeft}')
    expect(group?.getAttribute('aria-expanded')).toBe('false')
    expect(within(tree).queryByRole('button', { name: /Add export/ })).toBeNull()
    await user.keyboard('{ArrowRight}')
    expect(group?.getAttribute('aria-expanded')).toBe('true')
    await user.keyboard('{ArrowRight}')
    expect(document.activeElement?.getAttribute('data-roving')).toBe('task:T-0002')
    await user.keyboard('{ArrowLeft}')
    expect(document.activeElement).toBe(group)
  })

  it('lists three tasks that need the user and the rest behind "Show all"', async () => {
    installDaemon(
      ['T-0001', 'T-0002', 'T-0003', 'T-0004', 'T-0005'].map((id) =>
        makeTask({ id, title: `Review ${id}`, state: 'review' }),
      ),
    )
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')
    const strip = screen.getByRole('region', { name: 'Needs you' })

    expect(within(strip).getAllByRole('listitem')).toHaveLength(3)
    await user.click(within(strip).getByRole('button', { name: 'Show all 5' }))
    expect(within(strip).getAllByRole('listitem')).toHaveLength(5)
  })

  it('folds a section of the details and remembers it', async () => {
    installDaemon([makeTask({ id: 'T-0001', title: 'Fix it' })])
    const user = userEvent.setup()
    render(<App />)
    await signIn(user)
    await screen.findByRole('table')
    await user.click(within(screen.getByRole('table')).getByRole('button', { name: /Fix it/ }))
    const details = screen.getByRole('complementary', { name: 'Details of T-0001' })

    expect(within(details).getByText('No notes. Add some with Edit.')).toBeTruthy()
    await user.click(within(details).getByRole('button', { name: 'Notes' }))
    expect(within(details).queryByText('No notes. Add some with Edit.')).toBeNull()
    expect(window.localStorage.getItem('natamux.section.notes')).toBe('false')
  })
})
