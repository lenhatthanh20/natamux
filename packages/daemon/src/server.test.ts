import { existsSync, mkdirSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import type { FastifyInstance } from 'fastify'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { EventBus } from './event-bus.js'
import { createTestRepo, type TestRepo } from './git-repo.fixture.js'
import { buildServer, isAuthorized } from './server.js'
import { TaskIndex } from './task-index.js'
import { TaskService } from './task-service.js'
import { WorkspaceService } from './workspace-service.js'

const TOKEN = 'test-token'
const authorization = { authorization: `Bearer ${TOKEN}` }

let repo: TestRepo
let index: TaskIndex
let app: FastifyInstance

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
  app = await buildServer({ service, workspaces, bus, token: TOKEN })
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
