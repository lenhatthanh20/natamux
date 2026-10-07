import type { Task } from '@helmsman/core'
import { describe, expect, it } from 'vitest'
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
    env: options.env ?? { HELMSMAN_TOKEN: 'secret' },
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
      expect(result.stdout).toContain('Usage: helmsman')
      expect(result.stdout).toContain('agent.started')
      expect(result.calls).toEqual([])
    })

    it('exits with 2 and a hint for a wrong command line', async () => {
      const result = await run(['task', 'create'])
      expect(result.code).toBe(EXIT_USAGE)
      expect(result.stderr).toContain('A title is required')
      expect(result.stderr).toContain('helmsman help')
      expect(result.calls).toEqual([])
    })

    it('exits with 2 when the token is missing', async () => {
      const result = await run(['task', 'list'], { env: {} })
      expect(result.code).toBe(EXIT_USAGE)
      expect(result.stderr).toContain('HELMSMAN_TOKEN')
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

    it('uses HELMSMAN_URL and drops a trailing slash', async () => {
      const result = await run(['task', 'list'], {
        env: { HELMSMAN_TOKEN: 'secret', HELMSMAN_URL: 'http://example.test:9000/' },
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
        ],
        { handler: () => jsonResponse({ task: makeTask() }, 201) },
      )

      expect(result.code).toBe(EXIT_OK)
      expect(result.stdout).toBe('Created T-0001  Fix the dashboard\n')
      expect(result.calls[0]).toMatchObject({
        url: 'http://127.0.0.1:4317/tasks',
        method: 'POST',
        body: {
          title: 'Fix the dashboard',
          priority: 'high',
          labels: ['ui'],
          dependsOn: ['T-0007'],
        },
      })
    })

    it('prints JSON with --json', async () => {
      const task = makeTask()
      const result = await run(['task', 'create', 'Fix the dashboard', '--json'], {
        handler: () => jsonResponse({ task }, 201),
      })
      expect(JSON.parse(result.stdout)).toEqual(task)
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
      workspace: '/work/.helmsman/worktrees/T-0001',
      branch: 'helmsman/t-0001-fix-the-dashboard',
    })

    it('creates a workspace and prints its folder and branch', async () => {
      const result = await run(['workspace', 'create', 'T-0001'], {
        handler: () => jsonResponse({ task: withWorkspace }, 201),
      })

      expect(result.calls[0]).toMatchObject({
        url: 'http://127.0.0.1:4317/tasks/T-0001/workspace',
        method: 'POST',
      })
      expect(result.stdout).toContain('Created the workspace of T-0001')
      expect(result.stdout).toContain('helmsman/t-0001-fix-the-dashboard')
      expect(result.stdout).toContain('/work/.helmsman/worktrees/T-0001')
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
      expect(result.stdout).toBe('Removed the workspace of T-0001. The branch was deleted.\n')
    })

    it('says when the branch is kept', async () => {
      const result = await run(['workspace', 'remove', 'T-0001', '--yes'], {
        handler: () =>
          jsonResponse({
            task: makeTask({ workspace: null, branch: 'helmsman/t-0001-fix' }),
            branchDeleted: false,
          }),
      })

      expect(result.calls[0]?.url).toContain('deleteBranch=false')
      expect(result.stdout).toContain('The branch helmsman/t-0001-fix is kept.')
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
    it('shows the message of an unknown task', async () => {
      const result = await run(['task', 'show', 'T-0099'], {
        handler: () =>
          jsonResponse({ error: 'not_found', message: 'Task T-0099 does not exist' }, 404),
      })
      expect(result.code).toBe(EXIT_FAILED)
      expect(result.stderr).toBe('Error: Task T-0099 does not exist\n')
    })

    it('shows the issues of an invalid request', async () => {
      const result = await run(['task', 'create', 'x'], {
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
      expect(result.stderr).toContain('Check HELMSMAN_TOKEN')
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
