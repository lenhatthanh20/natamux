import type { Task } from '@helmsman/core'
import { describe, expect, it } from 'vitest'
import { ApiError, createApi, type FetchFunction, isUnauthorized } from './api'

const task: Task = {
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
}

type Call = { url: string; method: string; headers: Headers; body: unknown }

function fakeFetch(respond: () => Response) {
  const calls: Call[] = []
  const fetchFunction: FetchFunction = async (url, init) => {
    calls.push({
      url,
      method: init?.method ?? 'GET',
      headers: new Headers(init?.headers),
      body: typeof init?.body === 'string' ? JSON.parse(init.body) : undefined,
    })
    return respond()
  }
  return { calls, fetchFunction }
}

function json(data: unknown, status = 200): Response {
  return new Response(JSON.stringify(data), { status })
}

describe('createApi', () => {
  it('lists tasks through /api with the token', async () => {
    const { calls, fetchFunction } = fakeFetch(() => json({ tasks: [task] }))

    const tasks = await createApi('secret', fetchFunction).listTasks()

    expect(tasks).toEqual([task])
    expect(calls[0]?.url).toBe('/api/tasks')
    expect(calls[0]?.method).toBe('GET')
    expect(calls[0]?.headers.get('authorization')).toBe('Bearer secret')
  })

  it('creates a task from a title', async () => {
    const { calls, fetchFunction } = fakeFetch(() => json({ task }, 201))

    const created = await createApi('secret', fetchFunction).createTask('Example')

    expect(created).toEqual(task)
    expect(calls[0]).toMatchObject({
      url: '/api/tasks',
      method: 'POST',
      body: { title: 'Example' },
    })
    expect(calls[0]?.headers.get('content-type')).toBe('application/json')
  })

  it('sends an event as a human and reports whether it was accepted', async () => {
    const { calls, fetchFunction } = fakeFetch(() => json({ task, accepted: false }))

    const result = await createApi('secret', fetchFunction).sendEvent('T-0001', 'review.approved')

    expect(result).toEqual({ task, accepted: false })
    expect(calls[0]).toMatchObject({
      url: '/api/tasks/T-0001/events',
      method: 'POST',
      body: { source: 'human', type: 'review.approved' },
    })
  })

  it('creates a workspace for a task', async () => {
    const withWorkspace = { ...task, workspace: '/work/T-0001', branch: 'helmsman/t-0001-example' }
    const { calls, fetchFunction } = fakeFetch(() => json({ task: withWorkspace }, 201))

    const created = await createApi('secret', fetchFunction).createWorkspace('T-0001')

    expect(created).toEqual(withWorkspace)
    expect(calls[0]).toMatchObject({ url: '/api/tasks/T-0001/workspace', method: 'POST' })
  })

  it('removes a workspace and reports whether the branch was deleted', async () => {
    const { calls, fetchFunction } = fakeFetch(() => json({ task, branchDeleted: true }))

    const result = await createApi('secret', fetchFunction).removeWorkspace('T-0001')

    expect(result).toEqual({ task, branchDeleted: true })
    expect(calls[0]).toMatchObject({
      url: '/api/tasks/T-0001/workspace?deleteBranch=true',
      method: 'DELETE',
    })
  })

  it('throws an ApiError with the status when the daemon rejects the token', async () => {
    const { fetchFunction } = fakeFetch(() => json({ error: 'unauthorized' }, 401))

    const failure = await createApi('wrong', fetchFunction)
      .listTasks()
      .catch((error: unknown) => error)

    expect(failure).toBeInstanceOf(ApiError)
    expect(isUnauthorized(failure)).toBe(true)
  })

  it('uses the message of the daemon for other errors', async () => {
    const { fetchFunction } = fakeFetch(() =>
      json({ error: 'not_found', message: 'Task T-0099 does not exist' }, 404),
    )

    await expect(
      createApi('secret', fetchFunction).sendEvent('T-0099', 'task.readied'),
    ).rejects.toThrow('Task T-0099 does not exist')
  })

  it('says when the daemon cannot be reached', async () => {
    const fetchFunction: FetchFunction = async () => {
      throw new TypeError('Failed to fetch')
    }

    await expect(createApi('secret', fetchFunction).listTasks()).rejects.toThrow(
      'Cannot reach the daemon',
    )
  })

  it('rejects an answer that is not a task list', async () => {
    const { fetchFunction } = fakeFetch(() => json({ tasks: 'nope' }))
    await expect(createApi('secret', fetchFunction).listTasks()).rejects.toThrow(
      'does not understand',
    )
  })
})

describe('isUnauthorized', () => {
  it('is false for other errors', () => {
    expect(isUnauthorized(new ApiError(404, 'missing'))).toBe(false)
    expect(isUnauthorized(new Error('boom'))).toBe(false)
    expect(isUnauthorized(undefined)).toBe(false)
  })
})
