import { existsSync, mkdirSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import type { FastifyInstance } from 'fastify'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { AgentRunner } from './agent-runner.js'
import { EventBus } from './event-bus.js'
import { addBareOrigin, createTestRepo, type TestRepo } from './git-repo.fixture.js'
import { IssueImporter } from './issue-importer.js'
import { PullRequestWatcher } from './pull-request-watcher.js'
import { buildServer, isAuthorized } from './server.js'
import { TaskIndex } from './task-index.js'
import { TaskService } from './task-service.js'
import { TemplateService } from './template-service.js'
import { type FakePty, fakeSpawner } from './terminal-fake.fixture.js'
import { TerminalService } from './terminal-service.js'
import { WorkspaceService } from './workspace-service.js'

const TOKEN = 'test-token'
const authorization = { authorization: `Bearer ${TOKEN}` }

let repo: TestRepo
let index: TaskIndex
let app: FastifyInstance
let ptys: FakePty[]

beforeEach(async () => {
  repo = createTestRepo()
  const dataDir = join(repo.root, 'data')
  mkdirSync(dataDir)
  index = new TaskIndex(':memory:')
  const bus = new EventBus()
  const service = new TaskService({ dataDir, index, bus })
  const workspaces = new WorkspaceService({
    tasks: service,
    repoDir: repo.dir,
    worktreesDir: join(repo.root, 'worktrees'),
  })
  const spawner = fakeSpawner()
  ptys = spawner.ptys
  const terminals = new TerminalService({
    spawn: spawner.spawn,
    workspaceOf: (taskId) => workspaces.workspaceOf(taskId),
  })
  const agents = new AgentRunner({
    tasks: service,
    terminals,
    claudeSettingsPath: '/data/claude-code-settings.json',
    platform: 'linux',
  })
  const pullRequests = new PullRequestWatcher({
    tasks: service,
    cwd: repo.dir,
    gh: async () => JSON.stringify({ state: 'MERGED', isDraft: false, statusCheckRollup: [] }),
  })
  app = await buildServer({
    service,
    workspaces,
    terminals,
    agents,
    pullRequests,
    issues: new IssueImporter({
      tasks: service,
      cwd: repo.dir,
      gh: async () =>
        JSON.stringify([
          { number: 7, title: 'From GitHub', body: '', url: 'https://github.com/o/r/issues/7' },
        ]),
    }),
    templates: new TemplateService({ dataDir, tasks: service }),
    bus,
    token: TOKEN,
  })
})

afterEach(async () => {
  await app.close()
  index.close()
  repo.cleanup()
})

describe('isAuthorized', () => {
  it('accepts only the exact token', () => {
    expect(isAuthorized(TOKEN, TOKEN)).toBe(true)
    expect(isAuthorized('wrong-token', TOKEN)).toBe(false)
    expect(isAuthorized('short', TOKEN)).toBe(false)
    expect(isAuthorized(undefined, TOKEN)).toBe(false)
  })
})

describe('authentication', () => {
  it('serves /health without a token', async () => {
    const response = await app.inject({ method: 'GET', url: '/health' })
    expect(response.statusCode).toBe(200)
    expect(response.json()).toEqual({ status: 'ok' })
  })

  it('rejects a request without a token', async () => {
    const response = await app.inject({ method: 'GET', url: '/tasks' })
    expect(response.statusCode).toBe(401)
  })

  it('rejects a wrong token', async () => {
    const response = await app.inject({
      method: 'GET',
      url: '/tasks',
      headers: { authorization: 'Bearer nope' },
    })
    expect(response.statusCode).toBe(401)
  })

  it('does not accept the token in the query string outside /events', async () => {
    const response = await app.inject({ method: 'GET', url: `/tasks?token=${TOKEN}` })
    expect(response.statusCode).toBe(401)
  })
})

describe('tasks', () => {
  it('creates a task and reads it back', async () => {
    const created = await app.inject({
      method: 'POST',
      url: '/tasks',
      headers: authorization,
      payload: { title: 'Fix the dashboard' },
    })
    expect(created.statusCode).toBe(201)
    expect(created.json().task).toMatchObject({ id: 'T-0001', title: 'Fix the dashboard' })

    const fetched = await app.inject({
      method: 'GET',
      url: '/tasks/T-0001',
      headers: authorization,
    })
    expect(fetched.statusCode).toBe(200)
    expect(fetched.json().task.id).toBe('T-0001')

    const listed = await app.inject({ method: 'GET', url: '/tasks', headers: authorization })
    expect(listed.json().tasks).toHaveLength(1)
  })

  it('rejects an invalid task body with 400', async () => {
    const response = await app.inject({
      method: 'POST',
      url: '/tasks',
      headers: authorization,
      payload: { title: '' },
    })
    expect(response.statusCode).toBe(400)
    expect(response.json().error).toBe('invalid_request')
  })

  it('rejects a missing body with 400', async () => {
    const response = await app.inject({ method: 'POST', url: '/tasks', headers: authorization })
    expect(response.statusCode).toBe(400)
  })

  it('returns 404 for an unknown task', async () => {
    const response = await app.inject({
      method: 'GET',
      url: '/tasks/T-0099',
      headers: authorization,
    })
    expect(response.statusCode).toBe(404)
  })

  it('returns 400 for a task id that is not valid', async () => {
    const response = await app.inject({ method: 'GET', url: '/tasks/nope', headers: authorization })
    expect(response.statusCode).toBe(400)
  })

  it('rejects an unknown state filter with 400', async () => {
    const response = await app.inject({
      method: 'GET',
      url: '/tasks?state=finished',
      headers: authorization,
    })
    expect(response.statusCode).toBe(400)
  })
})

describe('task events', () => {
  async function createTask(): Promise<void> {
    await app.inject({
      method: 'POST',
      url: '/tasks',
      headers: authorization,
      payload: { title: 'Example' },
    })
  }

  it('applies an event and filters tasks by the new state', async () => {
    await createTask()

    const applied = await app.inject({
      method: 'POST',
      url: '/tasks/T-0001/events',
      headers: authorization,
      payload: { source: 'human', type: 'task.readied' },
    })
    expect(applied.statusCode).toBe(200)
    expect(applied.json()).toMatchObject({ accepted: true, task: { state: 'ready' } })

    const ready = await app.inject({
      method: 'GET',
      url: '/tasks?state=ready',
      headers: authorization,
    })
    expect(ready.json().tasks).toHaveLength(1)
  })

  it('reports an event that does not fit as not accepted', async () => {
    await createTask()

    const applied = await app.inject({
      method: 'POST',
      url: '/tasks/T-0001/events',
      headers: authorization,
      payload: { source: 'agent', type: 'agent.finished' },
    })
    expect(applied.statusCode).toBe(200)
    expect(applied.json()).toMatchObject({ accepted: false, task: { state: 'draft' } })
  })

  it('lists the events of a task', async () => {
    await createTask()
    await app.inject({
      method: 'POST',
      url: '/tasks/T-0001/events',
      headers: authorization,
      payload: { source: 'human', type: 'task.readied' },
    })

    const response = await app.inject({
      method: 'GET',
      url: '/tasks/T-0001/events',
      headers: authorization,
    })

    expect(
      response.json().events.map((entry: { event: { type: string } }) => entry.event.type),
    ).toEqual(['task.created', 'task.readied'])
  })

  it('rejects an unknown event type with 400', async () => {
    await createTask()
    const response = await app.inject({
      method: 'POST',
      url: '/tasks/T-0001/events',
      headers: authorization,
      payload: { source: 'agent', type: 'agent.exploded' },
    })
    expect(response.statusCode).toBe(400)
  })

  it('returns 404 for events on an unknown task', async () => {
    const response = await app.inject({
      method: 'POST',
      url: '/tasks/T-0099/events',
      headers: authorization,
      payload: { source: 'human', type: 'task.readied' },
    })
    expect(response.statusCode).toBe(404)
  })

  it.each(['task.created', 'workspace.created', 'workspace.removed', 'agent.active'])(
    'does not let a client send %s',
    async (type) => {
      await createTask()
      const response = await app.inject({
        method: 'POST',
        url: '/tasks/T-0001/events',
        headers: authorization,
        payload: { source: 'human', type },
      })
      expect(response.statusCode).toBe(400)
    },
  )
})

describe('workspaces', () => {
  async function createTask(): Promise<void> {
    await app.inject({
      method: 'POST',
      url: '/tasks',
      headers: authorization,
      payload: { title: 'Fix the bug' },
    })
  }

  it('creates a workspace and removes it again', async () => {
    await createTask()

    const created = await app.inject({
      method: 'POST',
      url: '/tasks/T-0001/workspace',
      headers: authorization,
    })
    expect(created.statusCode).toBe(201)
    const { workspace, branch } = created.json().task
    expect(branch).toBe('helmsman/t-0001-fix-the-bug')
    expect(existsSync(workspace)).toBe(true)

    const removed = await app.inject({
      method: 'DELETE',
      url: '/tasks/T-0001/workspace?deleteBranch=true',
      headers: authorization,
    })
    expect(removed.statusCode).toBe(200)
    expect(removed.json()).toMatchObject({
      branchDeleted: true,
      task: { workspace: null, branch: null },
    })
    expect(existsSync(workspace)).toBe(false)
  })

  it('answers 409 for a second workspace', async () => {
    await createTask()
    await app.inject({ method: 'POST', url: '/tasks/T-0001/workspace', headers: authorization })

    const again = await app.inject({
      method: 'POST',
      url: '/tasks/T-0001/workspace',
      headers: authorization,
    })

    expect(again.statusCode).toBe(409)
    expect(again.json().error).toBe('conflict')
  })

  it('answers 409 when there is no workspace to remove', async () => {
    await createTask()
    const response = await app.inject({
      method: 'DELETE',
      url: '/tasks/T-0001/workspace',
      headers: authorization,
    })
    expect(response.statusCode).toBe(409)
  })

  it('answers 404 for an unknown task', async () => {
    const response = await app.inject({
      method: 'POST',
      url: '/tasks/T-0099/workspace',
      headers: authorization,
    })
    expect(response.statusCode).toBe(404)
  })

  it('answers 400 for a deleteBranch value that is not true or false', async () => {
    await createTask()
    const response = await app.inject({
      method: 'DELETE',
      url: '/tasks/T-0001/workspace?deleteBranch=maybe',
      headers: authorization,
    })
    expect(response.statusCode).toBe(400)
  })

  it('needs the token', async () => {
    const response = await app.inject({ method: 'POST', url: '/tasks/T-0001/workspace' })
    expect(response.statusCode).toBe(401)
  })
})

describe('Claude Code hooks', () => {
  async function createTask(): Promise<void> {
    await app.inject({
      method: 'POST',
      url: '/tasks',
      headers: authorization,
      payload: { title: 'Fix the bug' },
    })
  }

  async function stateOf(taskId: string): Promise<string> {
    const response = await app.inject({
      method: 'GET',
      url: `/tasks/${taskId}`,
      headers: authorization,
    })
    return response.json().task.state
  }

  function post(payload: unknown, headers: Record<string, string> = {}) {
    return app.inject({
      method: 'POST',
      url: '/hooks/claude-code',
      headers: { ...authorization, ...headers },
      payload: payload as Record<string, unknown>,
    })
  }

  it('needs the token', async () => {
    const response = await app.inject({
      method: 'POST',
      url: '/hooks/claude-code',
      payload: { hook_event_name: 'Stop' },
    })
    expect(response.statusCode).toBe(401)
  })

  it('moves the task named in the header, and answers with an empty object', async () => {
    await createTask()

    const started = await post(
      { hook_event_name: 'UserPromptSubmit' },
      { 'x-helmsman-task': 'T-0001' },
    )
    expect(started.statusCode).toBe(200)
    expect(started.json()).toEqual({})
    expect(await stateOf('T-0001')).toBe('running')

    await post({ hook_event_name: 'Stop' }, { 'x-helmsman-task': 'T-0001' })
    expect(await stateOf('T-0001')).toBe('review')
  })

  it('finds the task by the workspace of the working directory', async () => {
    await createTask()
    const created = await app.inject({
      method: 'POST',
      url: '/tasks/T-0001/workspace',
      headers: authorization,
    })
    const workspace: string = created.json().task.workspace

    await post({ hook_event_name: 'UserPromptSubmit', cwd: `${workspace}/src` })

    expect(await stateOf('T-0001')).toBe('running')
  })

  it('answers 200 and changes nothing for a directory that belongs to no task', async () => {
    await createTask()

    const response = await post({ hook_event_name: 'UserPromptSubmit', cwd: '/elsewhere' })

    expect(response.statusCode).toBe(200)
    expect(await stateOf('T-0001')).toBe('draft')
  })

  it('answers 400 when the event name is missing', async () => {
    const response = await post({ cwd: '/elsewhere' })
    expect(response.statusCode).toBe(400)
  })

  it('answers 404, naming the known agents, for an agent it has no adapter for', async () => {
    const response = await app.inject({
      method: 'POST',
      url: '/hooks/nope',
      headers: authorization,
      payload: { hook_event_name: 'Stop' },
    })
    expect(response.statusCode).toBe(404)
    expect(response.json()).toMatchObject({ error: 'unknown_agent' })
    expect(response.json().message).toContain('claude-code')
  })

  it('answers 200 and changes nothing for a hook the adapter does not use', async () => {
    await createTask()
    const response = await post({ hook_event_name: 'SessionEnd' }, { 'x-helmsman-task': 'T-0001' })
    expect(response.statusCode).toBe(200)
    expect(await stateOf('T-0001')).toBe('draft')
  })
})

describe('changes', () => {
  async function taskWithWorkspace(): Promise<string> {
    await app.inject({
      method: 'POST',
      url: '/tasks',
      headers: authorization,
      payload: { title: 'Fix the bug' },
    })
    const created = await app.inject({
      method: 'POST',
      url: '/tasks/T-0001/workspace',
      headers: authorization,
    })
    return created.json().task.workspace
  }

  it('lists the changed files and gives both sides of one', async () => {
    const workspace = await taskWithWorkspace()
    writeFileSync(join(workspace, 'notes.txt'), 'hello\n')

    const list = await app.inject({
      method: 'GET',
      url: '/tasks/T-0001/changes',
      headers: authorization,
    })
    expect(list.statusCode).toBe(200)
    expect(list.json().changes).toEqual([{ path: 'notes.txt', status: 'U', originalPath: null }])

    const file = await app.inject({
      method: 'GET',
      url: '/tasks/T-0001/changes/file?path=notes.txt',
      headers: authorization,
    })
    expect(file.statusCode).toBe(200)
    expect(file.json()).toMatchObject({ path: 'notes.txt', original: null, modified: 'hello\n' })
  })

  it('answers 404 for a file that is not a changed file', async () => {
    await taskWithWorkspace()
    const response = await app.inject({
      method: 'GET',
      url: '/tasks/T-0001/changes/file?path=..%2F..%2Fetc%2Fpasswd',
      headers: authorization,
    })
    expect(response.statusCode).toBe(404)
  })

  it('answers 400 without a path', async () => {
    await taskWithWorkspace()
    const response = await app.inject({
      method: 'GET',
      url: '/tasks/T-0001/changes/file',
      headers: authorization,
    })
    expect(response.statusCode).toBe(400)
  })

  it('answers 409 for a task without a workspace, and 404 for an unknown task', async () => {
    await app.inject({
      method: 'POST',
      url: '/tasks',
      headers: authorization,
      payload: { title: 'Fix the bug' },
    })
    const none = await app.inject({
      method: 'GET',
      url: '/tasks/T-0001/changes',
      headers: authorization,
    })
    expect(none.statusCode).toBe(409)

    const unknown = await app.inject({
      method: 'GET',
      url: '/tasks/T-0099/changes',
      headers: authorization,
    })
    expect(unknown.statusCode).toBe(404)
  })

  it('needs the token', async () => {
    const response = await app.inject({ method: 'GET', url: '/tasks/T-0001/changes' })
    expect(response.statusCode).toBe(401)
  })
})

describe('graph', () => {
  it('gives the history of the task branch', async () => {
    await app.inject({
      method: 'POST',
      url: '/tasks',
      headers: authorization,
      payload: { title: 'Fix the bug' },
    })
    await app.inject({ method: 'POST', url: '/tasks/T-0001/workspace', headers: authorization })

    const response = await app.inject({
      method: 'GET',
      url: '/tasks/T-0001/graph',
      headers: authorization,
    })

    expect(response.statusCode).toBe(200)
    expect(response.json()).toMatchObject({ base: 'main', ahead: 0, behind: 0, truncated: false })
    expect(response.json().commits).toHaveLength(1)
  })

  it('answers 409 for a task without a workspace, and needs the token', async () => {
    await app.inject({
      method: 'POST',
      url: '/tasks',
      headers: authorization,
      payload: { title: 'Fix the bug' },
    })
    const none = await app.inject({
      method: 'GET',
      url: '/tasks/T-0001/graph',
      headers: authorization,
    })
    expect(none.statusCode).toBe(409)

    const anonymous = await app.inject({ method: 'GET', url: '/tasks/T-0001/graph' })
    expect(anonymous.statusCode).toBe(401)
  })
})

describe('terminal', () => {
  async function taskWithWorkspace(): Promise<void> {
    await app.inject({
      method: 'POST',
      url: '/tasks',
      headers: authorization,
      payload: { title: 'Fix the bug' },
    })
    await app.inject({ method: 'POST', url: '/tasks/T-0001/workspace', headers: authorization })
  }

  async function ticket(): Promise<string> {
    const response = await app.inject({
      method: 'POST',
      url: '/tasks/T-0001/terminal/ticket',
      headers: authorization,
    })
    expect(response.statusCode).toBe(200)
    return response.json().ticket
  }

  type Socket = Awaited<ReturnType<FastifyInstance['injectWS']>>

  function nextMessage(socket: Socket): Promise<unknown> {
    return new Promise((resolve) => {
      socket.once('message', (data: Buffer) => resolve(JSON.parse(data.toString())))
    })
  }

  function closeCode(socket: Socket): Promise<number> {
    return new Promise((resolve) => {
      socket.once('close', (code: number) => resolve(code))
    })
  }

  it('runs a shell in the workspace and passes input and output', async () => {
    await taskWithWorkspace()
    const socket = await app.injectWS(
      `/tasks/T-0001/terminal?ticket=${encodeURIComponent(await ticket())}&cols=100&rows=30`,
    )
    expect(ptys[0]?.options).toMatchObject({ cols: 100, rows: 30 })
    expect(ptys[0]?.options.cwd).toContain('T-0001')

    const echoed = nextMessage(socket)
    socket.send(JSON.stringify({ type: 'input', data: 'pwd\r' }))
    expect(await echoed).toEqual({ type: 'output', data: 'pwd\r' })

    socket.send(JSON.stringify({ type: 'resize', cols: 120, rows: 40 }))
    socket.send('not json')
    socket.send(JSON.stringify({ type: 'input', data: 42 }))
    // Frames arrive in order, so once this one is echoed, the ones before it were handled.
    const marker = nextMessage(socket)
    socket.send(JSON.stringify({ type: 'input', data: 'done' }))
    expect(await marker).toEqual({ type: 'output', data: 'done' })

    const exited = nextMessage(socket)
    const closed = closeCode(socket)
    ptys[0]?.exit(0)
    expect(await exited).toEqual({ type: 'exit', code: 0 })
    await closed
    expect(ptys[0]?.sizes).toEqual([{ cols: 120, rows: 40 }])
    expect(ptys[0]?.written).toEqual(['pwd\r', 'done'])
  })

  it('opens a second terminal, lists the terminals and closes one', async () => {
    await taskWithWorkspace()
    await app.injectWS(`/tasks/T-0001/terminal?ticket=${await ticket()}&cols=80&rows=24`)
    const second = await app.injectWS(
      `/tasks/T-0001/terminal?ticket=${await ticket()}&cols=80&rows=24&terminal=2`,
    )
    expect(ptys).toHaveLength(2)

    const listed = await app.inject({
      method: 'GET',
      url: '/tasks/T-0001/terminals',
      headers: authorization,
    })
    expect(listed.json()).toEqual({ terminals: ['1', '2'] })

    const closed = closeCode(second)
    const removed = await app.inject({
      method: 'DELETE',
      url: '/tasks/T-0001/terminals/2',
      headers: authorization,
    })
    expect(removed.json()).toEqual({ terminals: ['1'] })
    await closed

    const bad = await app.inject({
      method: 'DELETE',
      url: '/tasks/T-0001/terminals/0',
      headers: authorization,
    })
    expect(bad.statusCode).toBe(400)
  })

  it('sends a review comment to the agent in the first terminal', async () => {
    await taskWithWorkspace()
    const none = await app.inject({
      method: 'POST',
      url: '/tasks/T-0001/agent/message',
      headers: authorization,
      payload: { text: 'Rename the function' },
    })
    expect(none.statusCode).toBe(409)

    await app.injectWS(`/tasks/T-0001/terminal?ticket=${await ticket()}&cols=80&rows=24`)
    const sent = await app.inject({
      method: 'POST',
      url: '/tasks/T-0001/agent/message',
      headers: authorization,
      payload: { text: 'Rename the function' },
    })

    expect(sent.statusCode).toBe(200)
    expect(ptys[0]?.written).toEqual(['\x1b[200~Rename the function\x1b[201~\r'])
    const events = await app.inject({
      method: 'GET',
      url: '/tasks/T-0001/events',
      headers: authorization,
    })
    expect(events.json().events.at(-1).event).toMatchObject({
      type: 'review.commented',
      payload: { text: 'Rename the function' },
    })
  })

  it('closes a socket without a valid ticket', async () => {
    await taskWithWorkspace()
    const used = await ticket()
    const first = await app.injectWS(`/tasks/T-0001/terminal?ticket=${used}&cols=80&rows=24`)
    first.terminate()

    for (const query of [`ticket=${used}&cols=80&rows=24`, 'ticket=nope&cols=80&rows=24']) {
      const socket = await app.injectWS(`/tasks/T-0001/terminal?${query}`)
      expect(await closeCode(socket)).toBe(4401)
    }
    expect(ptys).toHaveLength(1)
  })

  it('does not let a ticket stand in for the token anywhere else', async () => {
    await taskWithWorkspace()
    const response = await app.inject({
      method: 'GET',
      url: `/tasks?ticket=${await ticket()}`,
    })
    expect(response.statusCode).toBe(401)
  })

  it('needs the token for a ticket, and a workspace', async () => {
    const anonymous = await app.inject({ method: 'POST', url: '/tasks/T-0001/terminal/ticket' })
    expect(anonymous.statusCode).toBe(401)

    await app.inject({
      method: 'POST',
      url: '/tasks',
      headers: authorization,
      payload: { title: 'Fix the bug' },
    })
    const none = await app.inject({
      method: 'POST',
      url: '/tasks/T-0001/terminal/ticket',
      headers: authorization,
    })
    expect(none.statusCode).toBe(409)
  })

  it('ends the shell before the workspace is removed', async () => {
    await taskWithWorkspace()
    const socket = await app.injectWS(
      `/tasks/T-0001/terminal?ticket=${await ticket()}&cols=80&rows=24`,
    )
    const closed = closeCode(socket)

    const removed = await app.inject({
      method: 'DELETE',
      url: '/tasks/T-0001/workspace',
      headers: authorization,
    })

    expect(removed.statusCode).toBe(200)
    expect(ptys[0]?.killed).toBe(true)
    await closed
  })
})

describe('commit and push', () => {
  async function taskWithChange(): Promise<string> {
    await app.inject({
      method: 'POST',
      url: '/tasks',
      headers: authorization,
      payload: { title: 'Fix the bug' },
    })
    const created = await app.inject({
      method: 'POST',
      url: '/tasks/T-0001/workspace',
      headers: authorization,
    })
    const workspace: string = created.json().task.workspace
    writeFileSync(join(workspace, 'fix.txt'), 'fixed\n')
    return workspace
  }

  it('commits the changes of the workspace', async () => {
    await taskWithChange()

    const response = await app.inject({
      method: 'POST',
      url: '/tasks/T-0001/commit',
      headers: authorization,
      payload: { message: '  Fix the bug  ' },
    })

    expect(response.statusCode).toBe(200)
    expect(response.json().hash).toMatch(/^[0-9a-f]{40}$/)
    expect(response.json().task.id).toBe('T-0001')

    const again = await app.inject({
      method: 'POST',
      url: '/tasks/T-0001/commit',
      headers: authorization,
      payload: { message: 'Again' },
    })
    expect(again.statusCode).toBe(409)
  })

  it('answers 400 for an empty message', async () => {
    await taskWithChange()
    const response = await app.inject({
      method: 'POST',
      url: '/tasks/T-0001/commit',
      headers: authorization,
      payload: { message: '   ' },
    })
    expect(response.statusCode).toBe(400)
  })

  it('pushes to origin, and answers 409 when there is none', async () => {
    await taskWithChange()
    const failed = await app.inject({
      method: 'POST',
      url: '/tasks/T-0001/push',
      headers: authorization,
    })
    expect(failed.statusCode).toBe(409)

    addBareOrigin(repo)
    const pushed = await app.inject({
      method: 'POST',
      url: '/tasks/T-0001/push',
      headers: authorization,
    })
    expect(pushed.statusCode).toBe(200)
    expect(pushed.json().task.id).toBe('T-0001')
  })

  it('needs the token', async () => {
    const urls = ['/tasks/T-0001/commit', '/tasks/T-0001/push', '/tasks/T-0001/pull-request']
    for (const url of urls) {
      const response = await app.inject({ method: 'POST', url })
      expect(response.statusCode).toBe(401)
    }
  })
})

describe('start an agent', () => {
  it('types the agent command into the task terminal', async () => {
    await app.inject({
      method: 'POST',
      url: '/tasks',
      headers: authorization,
      payload: { title: 'Fix the bug' },
    })
    await app.inject({ method: 'POST', url: '/tasks/T-0001/workspace', headers: authorization })

    const response = await app.inject({
      method: 'POST',
      url: '/tasks/T-0001/agent',
      headers: authorization,
      payload: { agent: 'claude-code' },
    })

    expect(response.statusCode).toBe(200)
    expect(response.json().task.agent).toBe('claude-code')
    expect(ptys[0]?.written[0]).toContain('claude --settings')
  })

  it('answers 404 for an agent it cannot start and 409 without a workspace', async () => {
    await app.inject({
      method: 'POST',
      url: '/tasks',
      headers: authorization,
      payload: { title: 'Fix the bug' },
    })
    const unknown = await app.inject({
      method: 'POST',
      url: '/tasks/T-0001/agent',
      headers: authorization,
      payload: { agent: 'nope' },
    })
    expect(unknown.statusCode).toBe(404)

    const none = await app.inject({
      method: 'POST',
      url: '/tasks/T-0001/agent',
      headers: authorization,
      payload: { agent: 'claude-code' },
    })
    expect(none.statusCode).toBe(409)
  })
})

describe('edit a task', () => {
  it('changes the details, and refuses bad changes and unknown tasks', async () => {
    await app.inject({
      method: 'POST',
      url: '/tasks',
      headers: authorization,
      payload: { title: 'Fix the bug' },
    })

    const edited = await app.inject({
      method: 'PATCH',
      url: '/tasks/T-0001',
      headers: authorization,
      payload: { title: 'Fix the parser', labels: ['parser'] },
    })
    expect(edited.statusCode).toBe(200)
    expect(edited.json().task).toMatchObject({ title: 'Fix the parser', labels: ['parser'] })

    const bad = await app.inject({
      method: 'PATCH',
      url: '/tasks/T-0001',
      headers: authorization,
      payload: { state: 'done' },
    })
    expect(bad.statusCode).toBe(400)

    const self = await app.inject({
      method: 'PATCH',
      url: '/tasks/T-0001',
      headers: authorization,
      payload: { dependsOn: ['T-0001'] },
    })
    expect(self.statusCode).toBe(400)

    const unknown = await app.inject({
      method: 'PATCH',
      url: '/tasks/T-0099',
      headers: authorization,
      payload: { title: 'x' },
    })
    expect(unknown.statusCode).toBe(404)
  })
})

describe('pull request status', () => {
  it('asks GitHub now for a task with a pull request', async () => {
    await app.inject({
      method: 'POST',
      url: '/tasks',
      headers: authorization,
      payload: { title: 'Fix the bug' },
    })
    const none = await app.inject({
      method: 'POST',
      url: '/tasks/T-0001/pull-request/refresh',
      headers: authorization,
    })
    expect(none.statusCode).toBe(200)
    expect(none.json().task.prStatus).toBeNull()
  })
})

describe('GitHub issues', () => {
  it('lists open issues and imports them as tasks', async () => {
    const list = await app.inject({ method: 'GET', url: '/github/issues', headers: authorization })
    expect(list.statusCode).toBe(200)
    expect(list.json().issues).toMatchObject([{ number: 7, title: 'From GitHub', taskId: null }])

    const imported = await app.inject({
      method: 'POST',
      url: '/github/issues/import',
      headers: authorization,
      payload: { numbers: [7] },
    })
    expect(imported.statusCode).toBe(200)
    expect(imported.json().tasks).toMatchObject([{ id: 'T-0001', title: 'From GitHub' }])
  })

  it('answers 400 without numbers', async () => {
    const response = await app.inject({
      method: 'POST',
      url: '/github/issues/import',
      headers: authorization,
      payload: { numbers: [] },
    })
    expect(response.statusCode).toBe(400)
  })
})

describe('templates', () => {
  it('saves a template and makes a task from it', async () => {
    const saved = await app.inject({
      method: 'POST',
      url: '/templates',
      headers: authorization,
      payload: { name: 'Bug fix', title: 'Fix', description: 'Steps', labels: ['bug'] },
    })
    expect(saved.statusCode).toBe(201)
    expect(saved.json().template.id).toBe('bug-fix')

    const listed = await app.inject({ method: 'GET', url: '/templates', headers: authorization })
    expect(listed.json().templates).toMatchObject([{ id: 'bug-fix', name: 'Bug fix' }])

    const created = await app.inject({
      method: 'POST',
      url: '/templates/bug-fix/tasks',
      headers: authorization,
      payload: { title: 'Fix the login' },
    })
    expect(created.statusCode).toBe(201)
    expect(created.json().task).toMatchObject({
      title: 'Fix the login',
      description: 'Steps',
      labels: ['bug'],
    })

    const missing = await app.inject({
      method: 'POST',
      url: '/templates/nope/tasks',
      headers: authorization,
      payload: { title: 'x' },
    })
    expect(missing.statusCode).toBe(404)
  })
})
