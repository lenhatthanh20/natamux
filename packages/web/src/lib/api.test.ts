import type { Task } from '@natamux/core'
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
  prStatus: null,
  stage: 'implement',
  stages: ['implement', 'review', 'release'],
  lanes: {},
  stageSince: null,
  backgroundSince: null,
  context: null,
  issue: null,
  parentId: null,
  repo: null,
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

  it('lists the events of a task', async () => {
    const entry = {
      seq: 3,
      accepted: false,
      event: {
        taskId: 'T-0001',
        source: 'agent',
        type: 'agent.finished',
        at: '2026-10-08T09:00:00.000Z',
        payload: {},
      },
    }
    const { calls, fetchFunction } = fakeFetch(() => json({ events: [entry] }))

    const events = await createApi('secret', fetchFunction).listEvents('T-0001')

    expect(events).toEqual([entry])
    expect(calls[0]).toMatchObject({ url: '/api/tasks/T-0001/events', method: 'GET' })
  })

  it('rejects an event list that is not a list', async () => {
    const { fetchFunction } = fakeFetch(() => json({ events: 'nope' }))
    await expect(createApi('secret', fetchFunction).listEvents('T-0001')).rejects.toThrow(
      'does not understand',
    )
  })

  it('creates a workspace for a task', async () => {
    const withWorkspace = { ...task, workspace: '/work/T-0001', branch: 'natamux/t-0001-example' }
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

  it('removes a repository', async () => {
    const { calls, fetchFunction } = fakeFetch(() => new Response(null, { status: 204 }))

    await createApi('secret', fetchFunction).removeRepo('web-app')

    expect(calls[0]).toMatchObject({ url: '/api/repos/web-app', method: 'DELETE' })
  })

  it('gives the reason when a repository cannot be removed', async () => {
    const { fetchFunction } = fakeFetch(() =>
      json({ error: 'conflict', message: 'Remove the 1 workspace of web-app first' }, 409),
    )

    await expect(createApi('secret', fetchFunction).removeRepo('web-app')).rejects.toThrow(
      'Remove the 1 workspace of web-app first',
    )
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

  it('commits, pushes and opens a pull request for a task', async () => {
    const withPr = { ...task, pr: 'https://github.com/o/r/pull/1' }
    const { calls, fetchFunction } = fakeFetch(() => json({ task: withPr }))
    const api = createApi('secret', fetchFunction)

    expect(await api.commitChanges('T-0001', 'Fix it')).toEqual(withPr)
    expect(await api.pushBranch('T-0001')).toEqual(withPr)
    expect(await api.openPullRequest('T-0001')).toEqual(withPr)
    expect(calls.map((call) => [call.method, call.url, call.body])).toEqual([
      ['POST', '/api/tasks/T-0001/commit', { message: 'Fix it' }],
      ['POST', '/api/tasks/T-0001/push', undefined],
      ['POST', '/api/tasks/T-0001/pull-request', undefined],
    ])
  })

  it('starts an agent for a task', async () => {
    const { calls, fetchFunction } = fakeFetch(() => json({ task }))

    const started = await createApi('secret', fetchFunction).startAgent('T-0001', 'claude-code')
    expect(started).toEqual(task)
    expect(calls[0]).toMatchObject({
      url: '/api/tasks/T-0001/agent',
      method: 'POST',
      body: { agent: 'claude-code' },
    })
  })

  it('asks for a terminal ticket with the token', async () => {
    const { calls, fetchFunction } = fakeFetch(() => json({ ticket: 'abc' }))

    expect(await createApi('secret', fetchFunction).terminalTicket('T-0001')).toBe('abc')
    expect(calls[0]).toMatchObject({ url: '/api/tasks/T-0001/terminal/ticket', method: 'POST' })
    expect(calls[0]?.headers.get('authorization')).toBe('Bearer secret')
  })

  it('reads the git graph of a task', async () => {
    const graph = {
      branch: 'natamux/t-0001',
      head: 'b2',
      base: 'main',
      ahead: 1,
      behind: 0,
      truncated: false,
      commits: [
        {
          hash: 'b2',
          parents: [],
          author: 'Ann',
          date: '2026-10-08T09:00:00.000Z',
          subject: 'init',
          refs: ['main'],
          onBranch: true,
        },
      ],
    }
    const { calls, fetchFunction } = fakeFetch(() => json(graph))

    expect(await createApi('secret', fetchFunction).getGitGraph('T-0001')).toEqual(graph)
    expect(calls[0]?.url).toBe('/api/tasks/T-0001/graph')
  })
})

describe('getFileImage', () => {
  it('sends the token and the encoded path and side, and gives the bytes', async () => {
    const headers = { 'content-type': 'image/png' }
    const png = () => new Response(new Uint8Array([1, 2]), { headers })
    const { calls, fetchFunction } = fakeFetch(png)
    const api = createApi('secret', fetchFunction)

    const image = await api.getFileImage('T-0001', 'assets/a b.png', 'original')

    expect(calls[0]?.url).toBe(
      '/api/tasks/T-0001/changes/image?path=assets%2Fa%20b.png&side=original',
    )
    expect(calls[0]?.headers.get('authorization')).toBe('Bearer secret')
    expect(image.type).toBe('image/png')
    expect([...new Uint8Array(await image.arrayBuffer())]).toEqual([1, 2])
  })

  it('turns a refusal into an ApiError with the status', async () => {
    const { fetchFunction } = fakeFetch(() =>
      json({ error: 'too_large', message: 'logo.png is larger than 5 MB' }, 413),
    )

    const failure = await createApi('secret', fetchFunction)
      .getFileImage('T-0001', 'logo.png', 'modified')
      .catch((error: unknown) => error)

    expect(failure).toBeInstanceOf(ApiError)
    expect((failure as ApiError).status).toBe(413)
    expect((failure as ApiError).message).toBe('logo.png is larger than 5 MB')
  })
})

describe('isUnauthorized', () => {
  it('is false for other errors', () => {
    expect(isUnauthorized(new ApiError(404, 'missing'))).toBe(false)
    expect(isUnauthorized(new Error('boom'))).toBe(false)
    expect(isUnauthorized(undefined)).toBe(false)
  })
})
