import { mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import type { FastifyInstance } from 'fastify'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { EventBus } from './event-bus.js'
import { buildServer, isAuthorized } from './server.js'
import { TaskIndex } from './task-index.js'
import { TaskService } from './task-service.js'

const TOKEN = 'test-token'
const authorization = { authorization: `Bearer ${TOKEN}` }

let dataDir: string
let index: TaskIndex
let app: FastifyInstance

beforeEach(async () => {
  dataDir = mkdtempSync(join(tmpdir(), 'helmsman-server-'))
  index = new TaskIndex(':memory:')
  const bus = new EventBus()
  const service = new TaskService({ dataDir, index, bus })
  app = await buildServer({ service, bus, token: TOKEN })
})

afterEach(async () => {
  await app.close()
  index.close()
  rmSync(dataDir, { recursive: true, force: true })
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
})
