import type { Task } from '@natamux/core'
import { describe, expect, it } from 'vitest'
import { ApiError, createApi, type FetchFunction, isConflict, isUnauthorized } from '../api'

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

  it('deletes a task and reads back the tasks that changed', async () => {
    const child = { ...task, id: 'T-0002', parentId: null, title: 'Notes' }
    const { calls, fetchFunction } = fakeFetch(() => json({ id: 'T-0001', tasks: [child] }))

    const result = await createApi('secret', fetchFunction).deleteTask('T-0001')

    expect(result).toEqual({ id: 'T-0001', tasks: [child] })
    expect(calls[0]).toMatchObject({ url: '/api/tasks/T-0001', method: 'DELETE' })
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

  it('removes a workspace and keeps its branch when asked to', async () => {
    const { calls, fetchFunction } = fakeFetch(() => json({ task, branchDeleted: false }))

    const result = await createApi('secret', fetchFunction).removeWorkspace('T-0001', false)

    expect(result).toEqual({ task, branchDeleted: false })
    expect(calls[0]?.url).toBe('/api/tasks/T-0001/workspace?deleteBranch=false')
  })

  it('makes a task from a template in the project it is given', async () => {
    const { calls, fetchFunction } = fakeFetch(() => json({ task }, 201))

    await createApi('secret', fetchFunction).createTaskFromTemplate('bug-fix', 'Fix it', 'web')

    expect(calls[0]).toMatchObject({
      url: '/api/templates/bug-fix/tasks',
      method: 'POST',
      body: { title: 'Fix it', repo: 'web' },
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

  it('rolls back changed files of a task, and undoes it', async () => {
    const { calls, fetchFunction } = fakeFetch(() => json({ files: 2 }))
    const api = createApi('secret', fetchFunction)

    expect(await api.rollbackFiles('T-0001', ['a.txt', 'b.txt'])).toBe(2)
    expect(await api.undoRollback('T-0001')).toBe(2)
    expect(calls.map((call) => [call.method, call.url, call.body])).toEqual([
      ['POST', '/api/tasks/T-0001/rollback', { paths: ['a.txt', 'b.txt'] }],
      ['POST', '/api/tasks/T-0001/rollback/undo', undefined],
    ])
  })

  it('asks for a commit message draft', async () => {
    const draft = { message: 'Fix it', source: 'draft', agent: 'codex', reason: 'codex timed out' }
    const { calls, fetchFunction } = fakeFetch(() => json(draft))

    expect(await createApi('secret', fetchFunction).generateCommitMessage('T-0001')).toEqual(draft)
    expect(calls.map((call) => [call.method, call.url, call.body])).toEqual([
      ['POST', '/api/tasks/T-0001/commit-message', undefined],
    ])
  })

  it('throws the message of a 409 and of a 404 when asked for a draft', async () => {
    const message = 'There is nothing to commit. The workspace matches its last commit.'
    const conflict = fakeFetch(() => json({ error: 'conflict', message }, 409))
    const draft = createApi('secret', conflict.fetchFunction).generateCommitMessage('T-0001')
    await expect(draft).rejects.toThrow(message)
    const missing = fakeFetch(() => json({ error: 'not_found', message: 'No task T-9' }, 404))
    const lost = createApi('secret', missing.fetchFunction).generateCommitMessage('T-9')
    await expect(lost).rejects.toBeInstanceOf(ApiError)
  })

  it.each([
    { source: 'agent', agent: 'codex' },
    { message: 'Fix it', source: 'other', agent: 'codex' },
  ])('rejects a draft of the wrong shape: %j', async (body) => {
    const { fetchFunction } = fakeFetch(() => json(body))
    await expect(createApi('secret', fetchFunction).generateCommitMessage('T-1')).rejects.toThrow()
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

  it('reopens the agent of a finished task', async () => {
    const { calls, fetchFunction } = fakeFetch(() => json({ task }))

    expect(await createApi('secret', fetchFunction).reopenAgent('T-0001')).toEqual(task)
    expect(calls[0]).toMatchObject({ url: '/api/tasks/T-0001/agent/reopen', method: 'POST' })
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
      push: { remote: 'tracked', unpushed: 1, unpulled: 0 },
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

  it('asks the daemon to read git now for a fresh graph or list of changes', async () => {
    // One body that reads as a graph and as a list of changes.
    const body = { head: 'b2', branch: null, base: null, ahead: null, behind: null }
    const { calls, fetchFunction } = fakeFetch(() =>
      json({ ...body, truncated: false, commits: [], changes: [] }),
    )
    const api = createApi('secret', fetchFunction)

    await api.getGitGraph('T-0001', true)
    await api.listChanges('T-0001', true)
    await api.listChanges('T-0001')

    expect(calls.map((call) => call.url)).toEqual([
      '/api/tasks/T-0001/graph?fresh=1',
      '/api/tasks/T-0001/changes?fresh=1',
      '/api/tasks/T-0001/changes',
    ])
  })

  it('reads a git graph without a push status as one whose status is unknown', async () => {
    const graph = {
      branch: 'natamux/t-0001',
      head: 'b2',
      base: 'main',
      ahead: 1,
      behind: 0,
      truncated: false,
      commits: [],
    }
    const { fetchFunction } = fakeFetch(() => json(graph))

    expect(await createApi('secret', fetchFunction).getGitGraph('T-0001')).toEqual({
      ...graph,
      push: null,
    })
  })

  it('reads the stage and state of each agent terminal', async () => {
    const { calls, fetchFunction } = fakeFetch(() =>
      json({
        terminals: ['1', '2', '3'],
        agents: ['1', '2'],
        lanes: {
          '1': { stage: 'implement', state: 'running', agent: 'claude-code' },
          '2': { stage: 'test-spec', state: 'review', agent: null },
        },
      }),
    )

    expect(await createApi('secret', fetchFunction).listAgentTerminals('T-0001')).toEqual({
      '1': { stage: 'implement', state: 'running', agent: 'claude-code' },
      '2': { stage: 'test-spec', state: 'review', agent: null },
    })
    expect(calls[0]?.url).toBe('/api/tasks/T-0001/terminals')
  })

  it('reads every shell of a task: its agent lane, and the program in front', async () => {
    const { fetchFunction } = fakeFetch(() =>
      json({
        terminals: ['1', '2', '3'],
        agents: ['1'],
        lanes: { '1': { stage: 'implement', state: 'running', agent: 'claude-code' } },
        programs: { '1': 'node', '2': 'zsh', '3': 7 },
      }),
    )

    expect(await createApi('secret', fetchFunction).listShells('T-0001')).toEqual({
      terminals: ['1', '2', '3'],
      lanes: { '1': { stage: 'implement', state: 'running', agent: 'claude-code' } },
      programs: { '1': 'node', '2': 'zsh', '3': null },
      restoring: {},
    })
  })

  it('reads the shells that come back after a restart, with their agents', async () => {
    const { fetchFunction } = fakeFetch(() =>
      json({
        terminals: ['1', '6', '7'],
        agents: [],
        lanes: {},
        programs: { '1': 'zsh' },
        restoring: { '6': 'copilot', '7': null, '8': 'cursor', '1': 5 },
      }),
    )

    const shells = await createApi('secret', fetchFunction).listShells('T-0001')

    // A shell the list does not have is dropped; an agent that is not text is a plain shell.
    expect(shells.restoring).toEqual({ '1': null, '6': 'copilot', '7': null })
  })

  it('reads what the agent of each terminal does, and the latest line of each task', async () => {
    const { fetchFunction } = fakeFetch(() =>
      json({
        activity: {
          'T-0001': {
            '1': { text: 'Edit src/a.ts', at: '2026-10-08T09:00:00.000Z' },
            '2': { text: 'Edit test-spec.md', at: '2026-10-08T09:00:05.000Z' },
          },
          'T-0002': { '1': { text: 'Bash: pnpm test', at: '2026-10-08T09:00:00.000Z' } },
        },
      }),
    )

    expect(await createApi('secret', fetchFunction).listActivity()).toEqual({
      byTask: { 'T-0001': 'Edit test-spec.md', 'T-0002': 'Bash: pnpm test' },
      byTerminal: {
        'T-0001': { '1': 'Edit src/a.ts', '2': 'Edit test-spec.md' },
        'T-0002': { '1': 'Bash: pnpm test' },
      },
    })
  })
})

describe('listPreparing', () => {
  it('reads the step of each workspace being made, and drops a step it does not know', async () => {
    const { calls, fetchFunction } = fakeFetch(() =>
      json({ preparing: { 'T-0001': 'fetch', 'T-0002': 'checkout', 'T-0003': 'later' } }),
    )

    expect(await createApi('secret', fetchFunction).listPreparing()).toEqual({
      'T-0001': 'fetch',
      'T-0002': 'checkout',
    })
    expect(calls[0]?.url).toBe('/api/workspaces/preparing')
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

describe('ended shells', () => {
  it('lists the last screens of the shells of a finished task', async () => {
    const shells = [
      {
        name: '1',
        agent: null,
        stage: null,
        session: null,
        endedAt: '2026-10-15T00:07:00.000Z',
        text: 'Done.',
      },
    ]
    const { calls, fetchFunction } = fakeFetch(() => json({ shells }))

    expect(await createApi('secret', fetchFunction).listEndedShells('T-0001')).toEqual(shells)
    expect(calls[0]?.url).toBe('/api/tasks/T-0001/ended-shells')
  })
})

describe('attachments', () => {
  it('lists the attachments of a task', async () => {
    const attachments = [{ name: 'a.png', size: 5, type: 'image/png' }]
    const { calls, fetchFunction } = fakeFetch(() => json({ attachments }))

    expect(await createApi('secret', fetchFunction).listAttachments('T-0001')).toEqual(attachments)
    expect(calls[0]?.url).toBe('/api/tasks/T-0001/attachments')
  })

  it('reads one attachment as bytes, by its encoded name', async () => {
    const { calls, fetchFunction } = fakeFetch(() => new Response(new Uint8Array([7])))

    const blob = await createApi('secret', fetchFunction).readAttachment('T-0001', 'a b.pdf')

    expect(calls[0]?.url).toBe('/api/tasks/T-0001/attachments/a%20b.pdf')
    expect([...new Uint8Array(await blob.arrayBuffer())]).toEqual([7])
  })

  it('sends the attachments of a new workspace in its JSON', async () => {
    const { calls, fetchFunction } = fakeFetch(() => json({ task }, 202))
    const api = createApi('secret', fetchFunction)
    const attachments = [{ name: 'a.png', data: 'AA==' }]

    await api.newWorkspace({ title: 'Fix it', attachments })

    expect(calls[0]?.url).toBe('/api/workspaces')
    expect(calls[0]?.body).toMatchObject({ title: 'Fix it', attachments })
  })
})

describe('project settings', () => {
  it('reads the text and the version of the file', async () => {
    const { calls, fetchFunction } = fakeFetch(() =>
      json({
        settings: {},
        error: null,
        found: true,
        path: '/work/.natamux.json',
        text: '{ "tracker": { "type": "none" }, "team": "web" }',
        version: 'abc123',
      }),
    )

    const loaded = await createApi('secret', fetchFunction).getSettings('web')

    expect(calls[0]?.url).toBe('/api/repos/web/settings')
    expect(loaded.text).toBe('{ "tracker": { "type": "none" }, "team": "web" }')
    expect(loaded.version).toBe('abc123')
    expect(loaded.found).toBe(true)
  })

  it('has no text and no version when the daemon sends none', async () => {
    const { fetchFunction } = fakeFetch(() => json({ settings: {}, found: false, path: '/w' }))

    const loaded = await createApi('secret', fetchFunction).getSettings('web')

    expect(loaded.text).toBeNull()
    expect(loaded.version).toBeNull()
  })

  it('sends the version it read with a save, and gives the new one', async () => {
    const { calls, fetchFunction } = fakeFetch(() => json({ settings: {}, version: 'def456' }))
    const raw = { tracker: { type: 'none' } }

    const saved = await createApi('secret', fetchFunction).saveSettings('web', raw, 'abc123')

    expect(calls[0]?.method).toBe('PUT')
    expect(calls[0]?.url).toBe('/api/repos/web/settings')
    expect(calls[0]?.headers.get('if-match')).toBe('abc123')
    expect(calls[0]?.body).toEqual(raw)
    expect(saved.version).toBe('def456')
    expect(saved.settings.tracker.type).toBe('github')
  })

  it('saves without a version when none is given, to overwrite the file', async () => {
    const { calls, fetchFunction } = fakeFetch(() => json({ settings: {}, version: 'def456' }))

    await createApi('secret', fetchFunction).saveSettings('web', {})

    expect(calls[0]?.headers.has('if-match')).toBe(false)
  })

  it('tells a file that changed on disk from other failures', async () => {
    const { fetchFunction } = fakeFetch(() =>
      json({ error: 'conflict', message: '/w/.natamux.json changed since you opened it.' }, 409),
    )

    const failure = await createApi('secret', fetchFunction)
      .saveSettings('web', {}, 'abc123')
      .catch((error: unknown) => error)

    expect(isConflict(failure)).toBe(true)
    expect(isConflict(new ApiError(400, 'bad'))).toBe(false)
    expect(isConflict(new Error('boom'))).toBe(false)
  })

  it('lists one folder of the worktree, the root by default', async () => {
    const entries = [{ name: 'src', kind: 'folder' }]
    const { calls, fetchFunction } = fakeFetch(() => json({ entries }))
    const api = createApi('secret', fetchFunction)

    expect(await api.listFolder('T-0001', '')).toEqual(entries)
    await api.listFolder('T-0001', 'packages/web src')

    expect(calls[0]?.url).toBe('/api/tasks/T-0001/tree?path=')
    expect(calls[1]?.url).toBe('/api/tasks/T-0001/tree?path=packages%2Fweb%20src')
  })

  it('reads a file of the worktree and saves it with the version it read', async () => {
    const file = { path: 'a.ts', text: 'x', binary: false, tooLarge: false, version: 'v1' }
    const { calls, fetchFunction } = fakeFetch(() =>
      calls.length === 1 ? json(file) : json({ path: 'a.ts', version: 'v2' }),
    )
    const api = createApi('secret', fetchFunction)

    expect(await api.readWorkspaceFile('T-0001', 'a.ts')).toEqual(file)
    expect(await api.writeWorkspaceFile('T-0001', 'a.ts', 'y', 'v1')).toBe('v2')

    expect(calls[0]?.url).toBe('/api/tasks/T-0001/file?path=a.ts')
    expect(calls[1]?.method).toBe('PUT')
    expect(calls[1]?.url).toBe('/api/tasks/T-0001/file?path=a.ts')
    expect(calls[1]?.headers.get('if-match')).toBe('v1')
    expect(calls[1]?.body).toEqual({ text: 'y' })
  })

  it('lists the skills of the agents, with the notices', async () => {
    const skill = {
      name: 'fetch-ticket',
      description: 'Reads a ticket',
      origin: 'project',
      agents: ['claude-code', 'codex'],
      path: '.claude/skills/fetch-ticket/SKILL.md',
    }
    const { calls, fetchFunction } = fakeFetch(() =>
      json({ skills: [skill], notices: ['The plugin tools is not installed on this machine.'] }),
    )

    const list = await createApi('secret', fetchFunction).getSkills('web')

    expect(calls[0]?.url).toBe('/api/repos/web/skills')
    expect(list).toEqual({
      skills: [skill],
      notices: ['The plugin tools is not installed on this machine.'],
    })
  })

  it('refuses a skill of an unknown origin', async () => {
    const { fetchFunction } = fakeFetch(() =>
      json({ skills: [{ name: 'x', origin: 'elsewhere' }], notices: [] }),
    )

    await expect(createApi('secret', fetchFunction).getSkills('web')).rejects.toThrow(
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
