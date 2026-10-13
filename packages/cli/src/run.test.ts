import { mkdtempSync, rmSync, truncateSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import type { Task } from '@natamux/core'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import type { FetchFunction } from './client.js'
import { EXIT_FAILED, EXIT_OK, EXIT_USAGE, runCli } from './run.js'

type Call = {
  url: string
  method: string
  headers: Record<string, string>
  body: unknown
}

type Handler = (call: Call) => Response

function makeTask(overrides: Partial<Task> = {}): Task {
  return {
    id: 'T-0001',
    title: 'Fix the dashboard',
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

function jsonResponse(data: unknown, status = 200): Response {
  return new Response(JSON.stringify(data), {
    status,
    headers: { 'content-type': 'application/json' },
  })
}

async function run(
  argv: string[],
  options: { env?: Record<string, string | undefined>; handler?: Handler } = {},
) {
  const calls: Call[] = []
  const fetchFunction: FetchFunction = async (url, init) => {
    const call: Call = {
      url,
      method: init?.method ?? 'GET',
      headers: Object.fromEntries(new Headers(init?.headers).entries()),
      body: typeof init?.body === 'string' ? JSON.parse(init.body) : undefined,
    }
    calls.push(call)
    return (options.handler ?? (() => jsonResponse({})))(call)
  }

  let stdout = ''
  let stderr = ''
  const code = await runCli(argv, {
    env: options.env ?? { NATAMUX_TOKEN: 'secret' },
    io: {
      stdout: (text) => {
        stdout += text
      },
      stderr: (text) => {
        stderr += text
      },
    },
    fetch: fetchFunction,
  })
  return { code, stdout, stderr, calls }
}

// `task create` without --repo asks for the projects first; with one project, it uses that one.
function oneProject(handler: Handler): Handler {
  return (call) =>
    call.url.endsWith('/repos')
      ? jsonResponse({ repos: [{ id: 'app', name: 'app', path: '/work/app' }] })
      : handler(call)
}

// Split every line of a printed table into its cells, so tests do not depend on the padding.
function cells(output: string): string[][] {
  return output
    .trimEnd()
    .split('\n')
    .map((line) => line.split(/ {2,}/))
}

describe('runCli', () => {
  describe('help and usage', () => {
    it('prints help without a token and without calling the daemon', async () => {
      const result = await run(['help'], { env: {} })
      expect(result.code).toBe(EXIT_OK)
      expect(result.stdout).toContain('Usage: natamux')
      expect(result.stdout).toContain('agent.started')
      expect(result.calls).toEqual([])
    })

    it('exits with 2 and a hint for a wrong command line', async () => {
      const result = await run(['task', 'create'])
      expect(result.code).toBe(EXIT_USAGE)
      expect(result.stderr).toContain('A title is required')
      expect(result.stderr).toContain('natamux help')
      expect(result.calls).toEqual([])
    })

    it('exits with 2 when the token is missing', async () => {
      const result = await run(['task', 'list'], { env: {} })
      expect(result.code).toBe(EXIT_USAGE)
      expect(result.stderr).toContain('NATAMUX_TOKEN')
      expect(result.calls).toEqual([])
    })
  })

  describe('requests', () => {
    it('sends the token and uses the default address', async () => {
      const result = await run(['task', 'list'], {
        handler: () => jsonResponse({ tasks: [] }),
      })
      expect(result.calls[0]).toMatchObject({
        url: 'http://127.0.0.1:4317/tasks',
        method: 'GET',
        headers: { authorization: 'Bearer secret' },
      })
    })

    it('uses NATAMUX_URL and drops a trailing slash', async () => {
      const result = await run(['task', 'list'], {
        env: { NATAMUX_TOKEN: 'secret', NATAMUX_URL: 'http://example.test:9000/' },
        handler: () => jsonResponse({ tasks: [] }),
      })
      expect(result.calls[0]?.url).toBe('http://example.test:9000/tasks')
    })
  })

  describe('task create', () => {
    it('sends the task and prints its id', async () => {
      const result = await run(
        [
          'task',
          'create',
          'Fix the dashboard',
          '--priority',
          'high',
          '--label',
          'ui',
          '--depends-on',
          'T-0007',
          '--repo',
          'web',
        ],
        { handler: () => jsonResponse({ task: makeTask() }, 201) },
      )

      expect(result.code).toBe(EXIT_OK)
      expect(result.stdout).toBe('Created T-0001  Fix the dashboard\n')
      expect(result.calls).toHaveLength(1)
      expect(result.calls[0]?.headers['content-type']).toBe('application/json')
      expect(result.calls[0]).toMatchObject({
        url: 'http://127.0.0.1:4317/tasks',
        method: 'POST',
        body: {
          title: 'Fix the dashboard',
          priority: 'high',
          labels: ['ui'],
          dependsOn: ['T-0007'],
          repo: 'web',
        },
      })
    })

    it('sends a title that starts with an id as typed, and prints the chosen id', async () => {
      const task = makeTask({ id: 'TASK-0006', title: 'x' })
      const result = await run(['task', 'create', 'Task-0006: x', '--repo', 'app'], {
        handler: () => jsonResponse({ task }, 201),
      })
      expect(result.code).toBe(EXIT_OK)
      expect(result.stdout).toBe('Created TASK-0006  x\n')
      expect(result.calls[0]?.body).toMatchObject({ title: 'Task-0006: x' })
    })

    it('prints JSON with --json', async () => {
      const task = makeTask()
      const result = await run(['task', 'create', 'Fix the dashboard', '--json'], {
        handler: oneProject(() => jsonResponse({ task }, 201)),
      })
      expect(JSON.parse(result.stdout)).toEqual(task)
    })

    it('puts the task in the only project when none is named', async () => {
      const result = await run(['task', 'create', 'Fix the dashboard'], {
        handler: oneProject(() => jsonResponse({ task: makeTask() }, 201)),
      })
      expect(result.code).toBe(EXIT_OK)
      expect(result.calls.map((call) => `${call.method} ${call.url}`)).toEqual([
        'GET http://127.0.0.1:4317/repos',
        'POST http://127.0.0.1:4317/tasks',
      ])
      expect(result.calls[1]?.body).toMatchObject({ repo: 'app' })
    })

    it('puts the task in the project of the terminal, from NATAMUX_TASK_ID', async () => {
      const result = await run(['task', 'create', 'Follow-up'], {
        env: { NATAMUX_TOKEN: 'secret', NATAMUX_TASK_ID: 'T-7' },
        handler: () => jsonResponse({ task: makeTask({ id: 'T-7', repo: 'web' }) }),
      })
      expect(result.code).toBe(EXIT_OK)
      expect(result.calls[0]?.url).toBe('http://127.0.0.1:4317/tasks/T-7')
      expect(result.calls[1]).toMatchObject({ method: 'POST', body: { repo: 'web' } })
    })

    it('asks for --repo and lists the projects when there are several', async () => {
      const result = await run(['task', 'create', 'Fix the dashboard'], {
        handler: () =>
          jsonResponse({
            repos: [
              { id: 'app', name: 'app', path: '/work/app' },
              { id: 'web', name: 'web', path: '/work/web' },
            ],
          }),
      })
      expect(result.code).toBe(EXIT_FAILED)
      expect(result.stderr).toBe('Error: Name the project with --repo. Projects: app, web\n')
      expect(result.calls.map((call) => call.method)).toEqual(['GET'])
    })

    describe('with --attach', () => {
      let dir: string

      beforeEach(() => {
        dir = mkdtempSync(join(tmpdir(), 'natamux-attach-'))
      })

      afterEach(() => {
        rmSync(dir, { recursive: true, force: true })
      })

      it('sends each file by its name, as base64', async () => {
        writeFileSync(join(dir, 'spec.pdf'), 'pdf')
        writeFileSync(join(dir, 'shot.png'), Buffer.from([0x89, 0x50, 0x4e, 0x47]))
        const files = ['--attach', join(dir, 'spec.pdf'), '--attach', join(dir, 'shot.png')]

        const result = await run(['task', 'create', 'Fix it', '--repo', 'web', ...files], {
          handler: () => jsonResponse({ task: makeTask() }, 201),
        })

        expect(result.code).toBe(EXIT_OK)
        expect(result.calls[0]?.body).toMatchObject({
          attachments: [
            { name: 'spec.pdf', data: Buffer.from('pdf').toString('base64') },
            { name: 'shot.png', data: Buffer.from([0x89, 0x50, 0x4e, 0x47]).toString('base64') },
          ],
        })
      })

      it('sends no attachments without --attach', async () => {
        const result = await run(['task', 'create', 'Fix it', '--repo', 'web'], {
          handler: () => jsonResponse({ task: makeTask() }, 201),
        })
        expect(result.calls[0]?.body).not.toHaveProperty('attachments')
      })

      it('makes no task when a file is missing', async () => {
        const missing = join(dir, 'missing.pdf')
        const result = await run(['task', 'create', 'Fix it', '--repo', 'web', '--attach', missing])

        expect(result.code).toBe(EXIT_FAILED)
        expect(result.stderr).toBe(`Error: There is no file at ${missing}\n`)
        expect(result.calls).toEqual([])
      })

      it('makes no task when a file is over the limit', async () => {
        const big = join(dir, 'big.mov')
        writeFileSync(big, '')
        truncateSync(big, 20_000_001)

        const result = await run(['task', 'create', 'Fix it', '--repo', 'web', '--attach', big])

        expect(result.code).toBe(EXIT_FAILED)
        expect(result.stderr).toContain('is larger than 20 MB, the most for one file')
        expect(result.calls).toEqual([])
      })
    })
  })

  describe('task list', () => {
    it('prints a table', async () => {
      const result = await run(['task', 'list'], {
        handler: () =>
          jsonResponse({
            tasks: [
              makeTask({ id: 'T-0002', title: 'Second', state: 'review', priority: 'high' }),
              makeTask({ id: 'T-0001', title: 'First' }),
            ],
          }),
      })

      expect(cells(result.stdout)).toEqual([
        ['ID', 'STATE', 'PRIORITY', 'TITLE'],
        ['T-0002', 'review', 'high', 'Second'],
        ['T-0001', 'draft', 'normal', 'First'],
      ])
    })

    it('says so when there are no tasks', async () => {
      const result = await run(['task', 'list'], { handler: () => jsonResponse({ tasks: [] }) })
      expect(result.stdout).toBe('No tasks.\n')
    })

    it('sends the state filter', async () => {
      const result = await run(['task', 'list', '--state', 'review'], {
        handler: () => jsonResponse({ tasks: [] }),
      })
      expect(result.calls[0]?.url).toBe('http://127.0.0.1:4317/tasks?state=review')
    })
  })

  describe('task show', () => {
    it('prints the fields and the description', async () => {
      const result = await run(['task', 'show', 'T-0001'], {
        handler: () =>
          jsonResponse({
            task: makeTask({ description: 'It is slow.', labels: ['ui', 'perf'], agent: 'claude' }),
          }),
      })

      expect(result.calls[0]?.url).toBe('http://127.0.0.1:4317/tasks/T-0001')
      expect(result.stdout).toContain('T-0001  Fix the dashboard')
      expect(result.stdout).toContain('Labels:     ui, perf')
      expect(result.stdout).toContain('Agent:      claude')
      expect(result.stdout).toContain('Branch:     -')
      expect(result.stdout).toContain('It is slow.')
    })

    it('shows the task of the terminal, from NATAMUX_TASK_ID', async () => {
      const result = await run(['task', 'show'], {
        env: { NATAMUX_TOKEN: 'secret', NATAMUX_TASK_ID: 'T-7' },
        handler: () => jsonResponse({ task: makeTask({ id: 'T-7' }) }),
      })

      expect(result.code).toBe(0)
      expect(result.calls[0]?.url).toBe('http://127.0.0.1:4317/tasks/T-7')
    })

    it('needs an id outside a task terminal', async () => {
      const result = await run(['task', 'show'])
      expect(result.code).toBe(2)
      expect(result.stderr).toContain('natamux task show T-42')
      expect(result.calls).toEqual([])
    })
  })

  describe('event', () => {
    it('sends the event and reports that it was accepted', async () => {
      const result = await run(['event', 'T-0001', 'task.readied'], {
        handler: () => jsonResponse({ task: makeTask({ state: 'ready' }), accepted: true }),
      })

      expect(result.stdout).toBe('Accepted. T-0001 is now ready.\n')
      expect(result.calls[0]).toMatchObject({
        url: 'http://127.0.0.1:4317/tasks/T-0001/events',
        method: 'POST',
        body: { source: 'human', type: 'task.readied', payload: {} },
      })
    })

    it('reports an event that was not accepted', async () => {
      const result = await run(['event', 'T-0001', 'agent.finished', '--source', 'agent'], {
        handler: () => jsonResponse({ task: makeTask(), accepted: false }),
      })
      expect(result.code).toBe(EXIT_OK)
      expect(result.stdout).toBe('Not accepted. T-0001 stays in draft.\n')
    })
  })

  describe('events', () => {
    it('prints the event log', async () => {
      const result = await run(['events', 'T-0001'], {
        handler: () =>
          jsonResponse({
            events: [
              {
                seq: 1,
                accepted: true,
                event: {
                  taskId: 'T-0001',
                  source: 'system',
                  type: 'task.created',
                  at: '2026-10-08T09:00:00.000Z',
                  payload: {},
                },
              },
              {
                seq: 2,
                accepted: false,
                event: {
                  taskId: 'T-0001',
                  source: 'agent',
                  type: 'agent.finished',
                  at: '2026-10-08T09:01:00.000Z',
                  payload: {},
                },
              },
            ],
          }),
      })

      expect(cells(result.stdout)).toEqual([
        ['SEQ', 'AT', 'SOURCE', 'TYPE', 'ACCEPTED'],
        ['1', '2026-10-08T09:00:00.000Z', 'system', 'task.created', 'yes'],
        ['2', '2026-10-08T09:01:00.000Z', 'agent', 'agent.finished', 'no'],
      ])
    })
  })

  describe('workspace', () => {
    const withWorkspace = makeTask({
      workspace: '/work/.natamux/worktrees/T-0001',
      branch: 'natamux/t-0001-fix-the-dashboard',
    })

    it('creates a workspace and prints its folder and branch', async () => {
      const result = await run(['workspace', 'create', 'T-0001'], {
        handler: () => jsonResponse({ task: withWorkspace }, 201),
      })

      expect(result.calls[0]).toMatchObject({
        url: 'http://127.0.0.1:4317/tasks/T-0001/workspace',
        method: 'POST',
      })
      // No body, so no JSON content type: the daemon answers 400 to that.
      expect(result.calls[0]?.headers['content-type']).toBeUndefined()
      expect(result.stdout).toContain('Created the workspace of T-0001')
      expect(result.stdout).toContain('natamux/t-0001-fix-the-dashboard')
      expect(result.stdout).toContain('/work/.natamux/worktrees/T-0001')
    })

    it('removes a workspace and says whether the branch was deleted', async () => {
      const result = await run(['workspace', 'remove', 'T-0001', '--yes', '--delete-branch'], {
        handler: () =>
          jsonResponse({ task: makeTask({ workspace: null, branch: null }), branchDeleted: true }),
      })

      expect(result.calls[0]).toMatchObject({
        url: 'http://127.0.0.1:4317/tasks/T-0001/workspace?deleteBranch=true',
        method: 'DELETE',
      })
      expect(result.calls[0]?.headers['content-type']).toBeUndefined()
      expect(result.stdout).toBe('Removed the workspace of T-0001. The branch was deleted.\n')
    })

    it('says when the branch is kept', async () => {
      const result = await run(['workspace', 'remove', 'T-0001', '--yes'], {
        handler: () =>
          jsonResponse({
            task: makeTask({ workspace: null, branch: 'natamux/t-0001-fix' }),
            branchDeleted: false,
          }),
      })

      expect(result.calls[0]?.url).toContain('deleteBranch=false')
      expect(result.stdout).toContain('The branch natamux/t-0001-fix is kept.')
    })

    it('does not call the daemon without --yes', async () => {
      const result = await run(['workspace', 'remove', 'T-0001'])
      expect(result.code).toBe(EXIT_USAGE)
      expect(result.calls).toEqual([])
    })

    it('shows the reason when git refuses', async () => {
      const result = await run(['workspace', 'remove', 'T-0001', '--yes'], {
        handler: () =>
          jsonResponse(
            { error: 'conflict', message: 'git worktree failed: contains modified files' },
            409,
          ),
      })
      expect(result.code).toBe(EXIT_FAILED)
      expect(result.stderr).toContain('contains modified files')
    })
  })

  describe('failures', () => {
    it('shows the message of a task id that is used', async () => {
      const result = await run(['task', 'create', 'Task-0005: Other', '--repo', 'app'], {
        handler: () =>
          jsonResponse(
            { error: 'invalid_request', message: 'Task id TASK-0005 is already used' },
            400,
          ),
      })
      expect(result.code).toBe(EXIT_FAILED)
      expect(result.stderr).toBe('Error: Task id TASK-0005 is already used\n')
    })

    it('shows the message of an unknown task', async () => {
      const result = await run(['task', 'show', 'T-0099'], {
        handler: () =>
          jsonResponse({ error: 'not_found', message: 'Task T-0099 does not exist' }, 404),
      })
      expect(result.code).toBe(EXIT_FAILED)
      expect(result.stderr).toBe('Error: Task T-0099 does not exist\n')
    })

    it('shows the issues of an invalid request', async () => {
      const result = await run(['task', 'create', 'x', '--repo', 'app'], {
        handler: () =>
          jsonResponse(
            {
              error: 'invalid_request',
              issues: [{ path: ['title'], message: 'Too short' }],
            },
            400,
          ),
      })
      expect(result.code).toBe(EXIT_FAILED)
      expect(result.stderr).toContain('title: Too short')
    })

    it('tells the user to check the token when the daemon answers 401', async () => {
      const result = await run(['task', 'list'], {
        handler: () => jsonResponse({ error: 'unauthorized' }, 401),
      })
      expect(result.code).toBe(EXIT_FAILED)
      expect(result.stderr).toContain('Check NATAMUX_TOKEN')
    })

    it('says when the daemon cannot be reached', async () => {
      const result = await run(['task', 'list'], {
        handler: () => {
          throw new TypeError('fetch failed')
        },
      })
      expect(result.code).toBe(EXIT_FAILED)
      expect(result.stderr).toContain('Cannot reach the daemon')
    })

    it('does not show a stack for an unexpected answer', async () => {
      const result = await run(['task', 'list'], { handler: () => jsonResponse(['nope']) })
      expect(result.code).toBe(EXIT_FAILED)
      expect(result.stderr).toContain('Unexpected answer from the daemon')
    })
  })
})

describe('more commands', () => {
  it('edits a task', async () => {
    const result = await run(['task', 'edit', 'T-1', '--title', 'New title'], {
      handler: () => jsonResponse({ task: makeTask({ id: 'T-1', title: 'New title' }) }),
    })

    expect(result.code).toBe(0)
    expect(result.calls[0]).toMatchObject({
      method: 'PATCH',
      url: 'http://127.0.0.1:4317/tasks/T-1',
      body: { title: 'New title' },
    })
    expect(result.stdout).toContain('New title')
  })

  it('starts an agent', async () => {
    const result = await run(['agent', 'start', 'T-1'], {
      handler: () => jsonResponse({ task: makeTask({ id: 'T-1' }) }),
    })

    expect(result.calls[0]).toMatchObject({
      method: 'POST',
      url: 'http://127.0.0.1:4317/tasks/T-1/agent',
      body: { agent: 'claude-code' },
    })
    expect(result.stdout).toBe('Started claude-code in the terminal of T-1\n')
  })

  it('sends the base branch for a new workspace with agent start', async () => {
    const result = await run(['agent', 'start', 'T-1', '--agent', 'codex', '--base', 'develop'], {
      handler: () => jsonResponse({ task: makeTask({ id: 'T-1' }) }),
    })

    expect(result.calls[0]).toMatchObject({ body: { agent: 'codex', base: 'develop' } })
  })

  it('asks for the task of the terminal, from NATAMUX_TASK_ID', async () => {
    const result = await run(['ask', 'Which database?'], {
      env: { NATAMUX_TOKEN: 'secret', NATAMUX_TASK_ID: 'T-7' },
      handler: () => jsonResponse({ task: makeTask({ id: 'T-7' }), accepted: true }),
    })

    expect(result.code).toBe(0)
    expect(result.calls[0]).toMatchObject({
      method: 'POST',
      url: 'http://127.0.0.1:4317/tasks/T-7/events',
      body: { source: 'agent', type: 'agent.asked', payload: { question: 'Which database?' } },
    })
  })

  it('names the terminal it runs in, so the daemon knows the stage inside a group', async () => {
    const env = { NATAMUX_TOKEN: 'secret', NATAMUX_TASK_ID: 'T-7', NATAMUX_TERMINAL: '2' }
    const handler = () => jsonResponse({ task: makeTask({ id: 'T-7' }), accepted: true })

    const ask = await run(['ask', 'Which file?'], { env, handler })
    expect(ask.calls[0]?.body).toEqual({
      source: 'agent',
      type: 'agent.asked',
      payload: { question: 'Which file?', terminal: '2' },
    })
    const done = await run(['done', '--stage', 'test-spec'], { env, handler })
    expect(done.calls[0]?.body).toEqual({ action: 'report', stage: 'test-spec', terminal: '2' })
    const approve = await run(['stage', 'approve', 'T-7'], { env, handler })
    expect(approve.calls[0]?.body).toEqual({ action: 'approve', terminal: '2' })
    // A terminal of another task says nothing about this one.
    const other = await run(['stage', 'approve', 'T-9'], { env, handler })
    expect(other.calls[0]?.body).toEqual({ action: 'approve' })
  })

  it('reports the stage done for the task named with --task', async () => {
    const result = await run(['done', '--task', 'T-8'], {
      handler: () => jsonResponse({ task: makeTask({ id: 'T-8' }) }),
    })
    expect(result.calls[0]).toMatchObject({
      url: 'http://127.0.0.1:4317/tasks/T-8/stage',
      body: { action: 'report' },
    })
  })

  it('approves a stage, and sends a task back with a reason', async () => {
    const approve = await run(['stage', 'approve', 'T-8', '--stage', 'test-spec'], {
      handler: () => jsonResponse({ task: makeTask({ id: 'T-8' }) }),
    })
    expect(approve.calls[0]).toMatchObject({
      url: 'http://127.0.0.1:4317/tasks/T-8/stage',
      body: { action: 'approve', stage: 'test-spec' },
    })
    const back = await run(['stage', 'back', 'T-8', '--to', 'plan', '--reason', 'Add tests'], {
      handler: () => jsonResponse({ task: makeTask({ id: 'T-8' }) }),
    })
    expect(back.calls[0]).toMatchObject({
      body: { action: 'reopen', to: 'plan', reason: 'Add tests' },
    })
  })

  it('turns the workflow of a stage on and off', async () => {
    const handler = () => jsonResponse({ task: makeTask({ id: 'T-8' }) })
    const on = await run(['stage', 'workflow', 'T-8', 'on', '--stage', 'implement'], { handler })
    expect(on.code).toBe(0)
    expect(on.calls[0]).toMatchObject({
      method: 'PUT',
      url: 'http://127.0.0.1:4317/tasks/T-8/workflow',
      body: { stage: 'implement', on: true },
    })
    expect(on.stdout).toContain('implement runs with a workflow the next time it starts')
    const off = await run(['stage', 'workflow', 'T-8', 'off', '--stage', 'implement'], { handler })
    expect(off.calls[0]).toMatchObject({ body: { stage: 'implement', on: false } })
    expect(off.stdout).toContain('implement runs without a workflow')
  })

  it('links an issue, and creates a task with its context source', async () => {
    const link = await run(
      ['task', 'link', 'T-8', '--issue', 'ABC-1', '--url', 'https://jira.example.com/ABC-1'],
      { handler: () => jsonResponse({ task: makeTask({ id: 'T-8' }) }) },
    )
    expect(link.calls[0]).toMatchObject({
      url: 'http://127.0.0.1:4317/tasks/T-8/issue',
      body: { provider: 'tracker', key: 'ABC-1' },
    })
    const create = await run(['task', 'create', 'Export', '--context', 'github-issue:42'], {
      handler: oneProject(() => jsonResponse({ task: makeTask({ id: 'T-9' }) }, 201)),
    })
    expect(create.calls.at(-1)).toMatchObject({
      body: { title: 'Export', context: { source: 'github-issue', ref: '42' } },
    })
  })

  it('needs a task for ask and done', async () => {
    const result = await run(['done'])
    expect(result.code).toBe(2)
    expect(result.stderr).toContain('NATAMUX_TASK_ID')
    expect(result.calls).toEqual([])
  })
})
