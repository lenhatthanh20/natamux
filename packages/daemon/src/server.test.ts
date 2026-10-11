import {
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  realpathSync,
  renameSync,
  rmSync,
  writeFileSync,
} from 'node:fs'
import { join } from 'node:path'
import type { FastifyInstance } from 'fastify'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { AgentRunner } from './agent-runner.js'
import { EventBus } from './event-bus.js'
import { worktreeState } from './git.js'
import { addBareOrigin, createTestRepo, runGit, type TestRepo } from './git-repo.fixture.js'
import { IssueImporter } from './issue-importer.js'
import { IssueLinker } from './issue-linker.js'
import { ProjectSettingsService } from './project-settings.js'
import { PullRequestWatcher } from './pull-request-watcher.js'
import { RepoService } from './repo-service.js'
import { buildServer, isAuthorized } from './server.js'
import { archiveDir } from './stage-files.js'
import { StageRunner } from './stage-runner.js'
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
let bus: EventBus
let webIndexPath: string
const PAGE = '<!doctype html><title>Natamux</title><div id="root"></div>'

beforeEach(async () => {
  repo = createTestRepo()
  const dataDir = join(repo.root, 'data')
  mkdirSync(dataDir)
  mkdirSync(join(repo.root, 'web'))
  webIndexPath = join(repo.root, 'web', 'index.html')
  writeFileSync(webIndexPath, PAGE)
  mkdirSync(join(repo.root, 'web', 'assets'))
  writeFileSync(join(repo.root, 'web', 'assets', 'index-abc.js'), 'console.log(1)\n')
  writeFileSync(join(repo.root, 'secret.txt'), 'not for the browser\n')
  index = new TaskIndex(':memory:')
  bus = new EventBus()
  const service = new TaskService({ dataDir, index, bus })
  const repos = new RepoService({
    dataDir,
    defaultPath: repo.dir,
    tasks: () => service.listTasks(),
  })
  const workspaces = new WorkspaceService({
    tasks: service,
    repoDir: repo.dir,
    worktreesDir: join(repo.root, 'worktrees'),
    repos,
    closeTerminals: (taskId) => terminals.close(taskId),
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
    launchDir: dataDir,
  })
  const pullRequests = new PullRequestWatcher({
    tasks: service,
    cwdOf: () => repo.dir,
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
      repoPath: () => repo.dir,
      gh: async () =>
        JSON.stringify([
          { number: 7, title: 'From GitHub', body: '', url: 'https://github.com/o/r/issues/7' },
        ]),
    }),
    templates: new TemplateService({ dataDir, tasks: service }),
    repos,
    bus,
    stages: new StageRunner({
      tasks: service,
      terminals,
      agents,
      bus,
      dataDir,
      repoPath: () => repo.dir,
    }),
    settings: new ProjectSettingsService({ repos }),
    issueLinker: new IssueLinker({ tasks: service, bus, dataDir, repoPath: () => repo.dir }),
    dataDir,
    token: TOKEN,
    webIndexPath,
    webDir: join(repo.root, 'web'),
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

describe('deep links', () => {
  const html = { accept: 'text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8' }

  it.each(['/', '/board', '/agents', '/t/T-0018', '/w/T-0018/changes?file=src/a.ts'])(
    'serves the dashboard page for %s, without a token',
    async (url) => {
      const response = await app.inject({ method: 'GET', url, headers: html })
      expect(response.statusCode).toBe(200)
      expect(response.headers['content-type']).toContain('text/html')
      expect(response.body).toBe(PAGE)
    },
  )

  it('still asks for the token on an API route, even when the browser asks for HTML', async () => {
    const response = await app.inject({ method: 'GET', url: '/tasks', headers: html })
    expect(response.statusCode).toBe(401)
    const withToken = await app.inject({
      method: 'GET',
      url: '/tasks',
      headers: { ...html, ...authorization },
    })
    expect(withToken.json()).toEqual({ tasks: [] })
  })

  it('answers 401, or 404 with a token, for an unknown address that is not HTML', async () => {
    const without = await app.inject({ method: 'GET', url: '/w/T-0018/changes' })
    expect(without.statusCode).toBe(401)
    const json = await app.inject({
      method: 'GET',
      url: '/w/T-0018/changes',
      headers: { ...authorization, accept: 'application/json' },
    })
    expect(json.statusCode).toBe(404)
    expect(json.json()).toMatchObject({ error: 'Not Found' })
  })

  it('does not serve the page for another method', async () => {
    const response = await app.inject({ method: 'POST', url: '/w/T-0018/changes', headers: html })
    expect(response.statusCode).toBe(401)
  })

  it('asks for the token again when the page file is gone', async () => {
    renameSync(webIndexPath, `${webIndexPath}.moved`)
    const response = await app.inject({ method: 'GET', url: '/board', headers: html })
    expect(response.statusCode).toBe(401)
  })
})

describe('the dashboard served by the daemon', () => {
  it('serves the built files without a token, and caches the hashed ones', async () => {
    const script = await app.inject({ method: 'GET', url: '/assets/index-abc.js' })
    expect(script.statusCode).toBe(200)
    expect(script.headers['content-type']).toContain('text/javascript')
    expect(script.headers['cache-control']).toContain('immutable')
    expect(script.body).toBe('console.log(1)\n')

    const page = await app.inject({ method: 'GET', url: '/index.html' })
    expect(page.headers['content-security-policy']).toContain("default-src 'self'")
  })

  it.each(['/../secret.txt', '/assets/..%2F..%2Fsecret.txt', '/%2e%2e/secret.txt'])(
    'serves nothing outside the folder for %s',
    async (url) => {
      const response = await app.inject({ method: 'GET', url })
      expect(response.statusCode).toBe(401)
    },
  )

  it('takes the API at /api too, as the dashboard asks for it', async () => {
    const response = await app.inject({ method: 'GET', url: '/api/tasks', headers: authorization })
    expect(response.json()).toEqual({ tasks: [] })
    const health = await app.inject({ method: 'GET', url: '/api/health' })
    expect(health.json()).toEqual({ status: 'ok' })
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

  it('deletes a task and the tasks that pointed at it stay', async () => {
    await app.inject({
      method: 'POST',
      url: '/tasks',
      headers: authorization,
      payload: { title: 'Release' },
    })
    await app.inject({
      method: 'POST',
      url: '/tasks',
      headers: authorization,
      payload: { title: 'Notes', parentId: 'T-0001' },
    })

    const deleted = await app.inject({
      method: 'DELETE',
      url: '/tasks/T-0001',
      headers: authorization,
    })

    expect(deleted.statusCode).toBe(200)
    expect(deleted.json().id).toBe('T-0001')
    expect(deleted.json().tasks).toMatchObject([{ id: 'T-0002', parentId: null }])
    const listed = await app.inject({ method: 'GET', url: '/tasks', headers: authorization })
    expect(listed.json().tasks.map((task: { id: string }) => task.id)).toEqual(['T-0002'])
    const gone = await app.inject({
      method: 'GET',
      url: '/tasks/T-0001',
      headers: authorization,
    })
    expect(gone.statusCode).toBe(404)
  })

  it('returns 404 when deleting an unknown task', async () => {
    const response = await app.inject({
      method: 'DELETE',
      url: '/tasks/T-0099',
      headers: authorization,
    })
    expect(response.statusCode).toBe(404)
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

  it('answers 400 for a send back to an unknown or later stage', async () => {
    await createTask()
    for (const to of ['nope', 'release']) {
      const response = await app.inject({
        method: 'POST',
        url: '/tasks/T-0001/stage',
        headers: authorization,
        payload: { action: 'reopen', to },
      })
      expect(response.statusCode).toBe(400)
      expect(response.json().message).toContain(`"${to}"`)
    }
  })

  it('refuses a new workspace for an agent the daemon cannot start', async () => {
    const response = await app.inject({
      method: 'POST',
      url: '/workspaces',
      headers: authorization,
      payload: { title: 'Example', agent: 'foo' },
    })
    expect(response.statusCode).toBe(400)
    const tasks = await app.inject({ method: 'GET', url: '/tasks', headers: authorization })
    expect(tasks.json().tasks).toEqual([])
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
    expect(branch).toBe('natamux/t-0001-fix-the-bug')
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

  it('removes a workspace whose folder was already deleted', async () => {
    await createTask()
    const created = await app.inject({
      method: 'POST',
      url: '/tasks/T-0001/workspace',
      headers: authorization,
    })
    rmSync(created.json().task.workspace, { recursive: true, force: true })
    runGit(repo.dir, 'worktree', 'prune')

    const removed = await app.inject({
      method: 'DELETE',
      url: '/tasks/T-0001/workspace',
      headers: authorization,
    })

    expect(removed.statusCode).toBe(200)
    expect(removed.json()).toMatchObject({
      branchDeleted: false,
      task: { workspace: null, branch: 'natamux/t-0001-fix-the-bug' },
    })
  })

  async function createWorkspace(): Promise<{ workspace: string; branch: string }> {
    await createTask()
    const created = await app.inject({
      method: 'POST',
      url: '/tasks/T-0001/workspace',
      headers: authorization,
    })
    return created.json().task
  }

  function removeWorkspace(query = '') {
    return app.inject({
      method: 'DELETE',
      url: `/tasks/T-0001/workspace${query}`,
      headers: authorization,
    })
  }

  it('removes a workspace whose folder was deleted with its merged branch', async () => {
    const { workspace } = await createWorkspace()
    rmSync(workspace, { recursive: true, force: true })
    runGit(repo.dir, 'worktree', 'prune')

    const removed = await removeWorkspace('?deleteBranch=true')

    expect(removed.statusCode).toBe(200)
    expect(removed.json()).toMatchObject({
      branchDeleted: true,
      task: { workspace: null, branch: null },
    })
  })

  it('removes a workspace whose folder was deleted while git still lists it', async () => {
    const { workspace, branch } = await createWorkspace()
    rmSync(workspace, { recursive: true, force: true })

    const removed = await removeWorkspace()

    expect(removed.statusCode).toBe(200)
    expect(removed.json()).toMatchObject({ task: { workspace: null, branch } })
    expect(await worktreeState(repo.dir, workspace)).toBe('missing')
  })

  it('answers 409 with the path for a folder that is not a git worktree', async () => {
    const { workspace } = await createWorkspace()
    rmSync(workspace, { recursive: true, force: true })
    runGit(repo.dir, 'worktree', 'prune')
    mkdirSync(workspace)

    const removed = await removeWorkspace()

    expect(removed.statusCode).toBe(409)
    expect(removed.json().message).toContain(workspace)
    expect(existsSync(workspace)).toBe(true)
  })

  it('answers 409 for a worktree with changes that are not committed', async () => {
    const { workspace } = await createWorkspace()
    writeFileSync(join(workspace, 'notes.txt'), 'work in progress')

    const removed = await removeWorkspace()

    expect(removed.statusCode).toBe(409)
    expect(existsSync(join(workspace, 'notes.txt'))).toBe(true)
  })

  it('deletes a task and its worktree, and keeps both when the worktree has changes', async () => {
    const { workspace } = await createWorkspace()
    writeFileSync(join(workspace, 'notes.txt'), 'work in progress')

    const refused = await app.inject({
      method: 'DELETE',
      url: '/tasks/T-0001',
      headers: authorization,
    })
    expect(refused.statusCode).toBe(409)
    expect(existsSync(join(workspace, 'notes.txt'))).toBe(true)
    const stillThere = await app.inject({
      method: 'GET',
      url: '/tasks/T-0001',
      headers: authorization,
    })
    expect(stillThere.statusCode).toBe(200)

    rmSync(join(workspace, 'notes.txt'))
    const deleted = await app.inject({
      method: 'DELETE',
      url: '/tasks/T-0001',
      headers: authorization,
    })
    expect(deleted.statusCode).toBe(200)
    expect(existsSync(workspace)).toBe(false)
    const gone = await app.inject({
      method: 'GET',
      url: '/tasks/T-0001',
      headers: authorization,
    })
    expect(gone.statusCode).toBe(404)
  })

  it('keeps the earlier stage files when the folder was already deleted', async () => {
    const { workspace } = await createWorkspace()
    const archive = archiveDir(join(repo.root, 'data'), 'T-0001')
    mkdirSync(archive, { recursive: true })
    writeFileSync(join(archive, 'spec.md'), '# Goal\nKeep me.\n')
    rmSync(workspace, { recursive: true, force: true })

    const removed = await removeWorkspace()

    expect(removed.statusCode).toBe(200)
    expect(readFileSync(join(archive, 'spec.md'), 'utf8')).toBe('# Goal\nKeep me.\n')
  })

  it('archives nothing from a folder that was already deleted', async () => {
    const { workspace } = await createWorkspace()
    const archive = archiveDir(join(repo.root, 'data'), 'T-0001')
    const archived = () => (existsSync(archive) ? readdirSync(archive) : [])
    const before = archived()
    rmSync(workspace, { recursive: true, force: true })

    const removed = await removeWorkspace()

    expect(removed.statusCode).toBe(200)
    expect(archived()).toEqual(before)
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
      { 'x-natamux-task': 'T-0001' },
    )
    expect(started.statusCode).toBe(200)
    expect(started.json()).toEqual({})
    expect(await stateOf('T-0001')).toBe('running')

    await post({ hook_event_name: 'Stop' }, { 'x-natamux-task': 'T-0001' })
    expect(await stateOf('T-0001')).toBe('needs-input')
  })

  it('keeps the task running when the agent stops with background work', async () => {
    await createTask()
    const header = { 'x-natamux-task': 'T-0001' }
    await post({ hook_event_name: 'UserPromptSubmit' }, header)

    const response = await post(
      {
        hook_event_name: 'Stop',
        background_tasks: [{ id: 'b1', type: 'shell', status: 'running', command: 'pnpm test' }],
        session_crons: [],
      },
      header,
    )

    expect(response.statusCode).toBe(200)
    expect(await stateOf('T-0001')).toBe('running')
  })

  it.each<[unknown, unknown]>([
    [null, null],
    ['x', 'x'],
    [42, 42],
    [{}, {}],
    [
      [null, 7, { status: 'running' }],
      [null, 'daily'],
    ],
  ])(
    'answers 200 and ends the turn as before for the background work %j and crons %j',
    async (backgroundTasks, sessionCrons) => {
      await createTask()
      const header = { 'x-natamux-task': 'T-0001' }
      await post({ hook_event_name: 'UserPromptSubmit' }, header)

      const response = await post(
        { hook_event_name: 'Stop', background_tasks: backgroundTasks, session_crons: sessionCrons },
        header,
      )

      expect(response.statusCode).toBe(200)
      expect(await stateOf('T-0001')).toBe('needs-input')
    },
  )

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
    const response = await post({ hook_event_name: 'SessionEnd' }, { 'x-natamux-task': 'T-0001' })
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
    expect(list.json().changes).toEqual([
      { path: 'notes.txt', status: 'U', originalPath: null, added: 1, removed: 0 },
    ])

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

describe('changed images', () => {
  const PNG = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0])

  async function workspaceWithImage(): Promise<string> {
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
    writeFileSync(join(workspace, 'logo.png'), PNG)
    return workspace
  }

  const image = (query: string) =>
    app.inject({
      method: 'GET',
      url: `/tasks/T-0001/changes/image?${query}`,
      headers: authorization,
    })

  it('sends the bytes with a type, nosniff and a policy that runs nothing', async () => {
    await workspaceWithImage()

    const response = await image('path=logo.png&side=modified')

    expect(response.statusCode).toBe(200)
    expect(response.headers['content-type']).toBe('image/png')
    expect(response.headers['x-content-type-options']).toBe('nosniff')
    expect(response.headers['cache-control']).toBe('no-store')
    expect(response.headers['content-security-policy']).toBe(
      "default-src 'none'; style-src 'unsafe-inline'; sandbox",
    )
    expect([...response.rawPayload]).toEqual([...PNG])
  })

  it('answers 400 for an unknown side or a file that is not an image', async () => {
    const workspace = await workspaceWithImage()
    writeFileSync(join(workspace, 'notes.txt'), 'hello\n')

    expect((await image('path=logo.png&side=both')).statusCode).toBe(400)
    expect((await image('path=notes.txt&side=modified')).statusCode).toBe(400)
  })

  it('answers 404 for a side that is not there, or a file that did not change', async () => {
    await workspaceWithImage()

    expect((await image('path=logo.png&side=original')).statusCode).toBe(404)
    expect((await image('path=other.png&side=modified')).statusCode).toBe(404)
  })

  it('answers 413 for an image that is too large', async () => {
    const workspace = await workspaceWithImage()
    writeFileSync(join(workspace, 'big.png'), Buffer.alloc(5_000_001))

    const response = await image('path=big.png&side=modified')

    expect(response.statusCode).toBe(413)
    expect(response.json().error).toBe('too_large')
  })
})

describe('attachments', () => {
  const PNG = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0])
  const file = (name: string, bytes: Buffer | string) => ({
    name,
    data: Buffer.from(bytes).toString('base64'),
  })

  const newWorkspace = (attachments: unknown[]) =>
    app.inject({
      method: 'POST',
      url: '/workspaces',
      headers: authorization,
      payload: { title: 'Fix the login', attachments },
    })

  const attachment = (name: string) =>
    app.inject({
      method: 'GET',
      url: `/tasks/T-0001/attachments/${encodeURIComponent(name)}`,
      headers: authorization,
    })

  it('keeps the files sent with a new workspace and lists them', async () => {
    const created = await newWorkspace([
      file('login error.png', PNG),
      file('spec.docx', 'word'),
      file('spec.docx', 'word again'),
    ])
    expect(created.statusCode).toBe(201)

    const listed = await app.inject({
      method: 'GET',
      url: '/tasks/T-0001/attachments',
      headers: authorization,
    })
    expect(listed.json().attachments).toEqual([
      { name: 'login-error.png', size: PNG.length, type: 'image/png' },
      { name: 'spec-2.docx', size: 10, type: null },
      { name: 'spec.docx', size: 4, type: null },
    ])
    const events = await app.inject({
      method: 'GET',
      url: '/tasks/T-0001/events',
      headers: authorization,
    })
    const types = events.json().events.map((entry: { event: { type: string } }) => entry.event.type)
    expect(types.filter((type: string) => type === 'attachment.added')).toHaveLength(3)
  })

  it('keeps the files sent with a new task', async () => {
    const created = await app.inject({
      method: 'POST',
      url: '/tasks',
      headers: authorization,
      payload: { title: 'Fix the login', attachments: [file('notes.md', '# Notes')] },
    })
    expect(created.statusCode).toBe(201)

    expect((await attachment('notes.md')).body).toBe('# Notes')
  })

  it('sends an image to show, with nosniff and a policy that runs nothing', async () => {
    await newWorkspace([file('shot.png', PNG)])

    const response = await attachment('shot.png')

    expect(response.statusCode).toBe(200)
    expect(response.headers['content-type']).toBe('image/png')
    expect(response.headers['content-disposition']).toBeUndefined()
    expect(response.headers['x-content-type-options']).toBe('nosniff')
    expect(response.headers['content-security-policy']).toContain('sandbox')
    expect([...response.rawPayload]).toEqual([...PNG])
  })

  it.each(['page.html', 'logo.svg', 'report.pdf'])('sends %s only as a download', async (name) => {
    await newWorkspace([file(name, '<script>alert(1)</script>')])

    const response = await attachment(name)

    expect(response.statusCode).toBe(200)
    expect(response.headers['content-type']).toBe('application/octet-stream')
    expect(response.headers['content-disposition']).toBe(`attachment; filename="${name}"`)
    expect(response.headers['x-content-type-options']).toBe('nosniff')
  })

  it('answers 404 for a file the task lacks, and 400 for a name that is not safe', async () => {
    await newWorkspace([file('a.txt', 'a')])

    expect((await attachment('b.txt')).statusCode).toBe(404)
    expect((await attachment('.hidden')).statusCode).toBe(400)
    expect((await attachment('a b.txt')).statusCode).toBe(400)
    const unknown = await app.inject({
      method: 'GET',
      url: '/tasks/T-0042/attachments',
      headers: authorization,
    })
    expect(unknown.statusCode).toBe(404)
  })

  it('keeps the files after the workspace is removed', async () => {
    await newWorkspace([file('a.txt', 'a')])
    await app.inject({ method: 'DELETE', url: '/tasks/T-0001/workspace', headers: authorization })

    expect((await attachment('a.txt')).body).toBe('a')
  })

  it('refuses more than 20 files and makes no task', async () => {
    const files = Array.from({ length: 21 }, (_, n) => file(`f${n}.txt`, 'x'))

    expect((await newWorkspace(files)).statusCode).toBe(400)
    const tasks = await app.inject({ method: 'GET', url: '/tasks', headers: authorization })
    expect(tasks.json().tasks).toEqual([])
  })

  it('takes a body larger than 1 MB when it brings files', async () => {
    const response = await newWorkspace([file('big.bin', Buffer.alloc(2_000_000))])

    expect(response.statusCode).toBe(201)
    expect((await attachment('big.bin')).rawPayload.length).toBe(2_000_000)
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
    expect(response.json()).toMatchObject({
      base: 'main',
      ahead: 0,
      behind: 0,
      push: { remote: 'none' },
      truncated: false,
    })
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
    expect(listed.json()).toEqual({ terminals: ['1', '2'], agents: [], lanes: {} })
    // A program that is not a shell runs in the first one: an agent, as far as Natamux can tell.
    ;(ptys[0] as FakePty).program = 'node'
    const withAgent = await app.inject({
      method: 'GET',
      url: '/tasks/T-0001/terminals',
      headers: authorization,
    })
    const read = await app.inject({ method: 'GET', url: '/tasks/T-0001', headers: authorization })
    const task = read.json().task
    // The first terminal has the task's stage and state; the second has no agent, so no lane.
    expect(withAgent.json()).toEqual({
      terminals: ['1', '2'],
      agents: ['1'],
      lanes: { '1': { stage: task.stage, state: task.state } },
    })
    // An agent in an extra shell outside a group works on no stage.
    ;(ptys[1] as FakePty).program = 'node'
    const twoAgents = await app.inject({
      method: 'GET',
      url: '/tasks/T-0001/terminals',
      headers: authorization,
    })
    expect(twoAgents.json().lanes['2']).toEqual({ stage: null, state: null })

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
    // Only a shell runs: the comment would run as commands there.
    const shell = await app.inject({
      method: 'POST',
      url: '/tasks/T-0001/agent/message',
      headers: authorization,
      payload: { text: 'Rename the function' },
    })
    expect(shell.statusCode).toBe(409)
    expect(ptys[0]?.written).toEqual([])

    const agent = ptys[0] as FakePty
    agent.program = 'node'
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
    expect(ptys[0]?.written[0]).toContain('agent-launch-')
  })

  it('answers 404 for an agent it cannot start, and makes a missing workspace', async () => {
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
    const before = await app.inject({ method: 'GET', url: '/tasks/T-0001', headers: authorization })
    expect(before.json().task.workspace).toBeNull()

    const started = await app.inject({
      method: 'POST',
      url: '/tasks/T-0001/agent',
      headers: authorization,
      payload: { agent: 'claude-code' },
    })
    expect(started.statusCode).toBe(200)
    expect(started.json().task.workspace).not.toBeNull()
    expect(ptys[0]?.written.join('')).toContain('agent-launch-')
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

    const elsewhere = await app.inject({
      method: 'POST',
      url: '/templates/bug-fix/tasks',
      headers: authorization,
      payload: { title: 'x', repo: 'nope' },
    })
    expect(elsewhere.statusCode).toBe(404)
    expect(elsewhere.json().message).toBe('Repository nope is not known. Add it first')
  })
})

describe('agent activity', () => {
  it('lists what the agents do now', async () => {
    await app.inject({
      method: 'POST',
      url: '/tasks',
      headers: authorization,
      payload: { title: 'Fix the bug' },
    })
    // Hooks from a terminal count only while the task has its workspace.
    await app.inject({ method: 'POST', url: '/tasks/T-0001/workspace', headers: authorization })
    await app.inject({
      method: 'POST',
      url: '/hooks/claude-code',
      headers: { ...authorization, 'x-natamux-task': 'T-0001' },
      payload: {
        hook_event_name: 'PreToolUse',
        tool_name: 'Edit',
        tool_input: { file_path: 'a.ts' },
      },
    })

    await app.inject({
      method: 'POST',
      url: '/hooks/claude-code',
      headers: { ...authorization, 'x-natamux-task': 'T-0001', 'x-natamux-terminal': '2' },
      payload: {
        hook_event_name: 'PreToolUse',
        tool_name: 'Edit',
        tool_input: { file_path: 'test-spec.md' },
      },
    })

    const response = await app.inject({ method: 'GET', url: '/activity', headers: authorization })

    const activity = response.json().activity['T-0001']
    expect(activity['1'].text).toBe('Edit a.ts')
    expect(activity['2'].text).toBe('Edit test-spec.md')
  })
})

describe('branches', () => {
  it('lists branches and makes a workspace from one', async () => {
    const listed = await app.inject({ method: 'GET', url: '/git/branches', headers: authorization })
    expect(listed.json()).toEqual({ branches: ['main'], current: 'main' })

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
      payload: { base: 'main' },
    })
    expect(created.statusCode).toBe(201)

    await app.inject({
      method: 'POST',
      url: '/tasks',
      headers: authorization,
      payload: { title: 'Another' },
    })
    const missing = await app.inject({
      method: 'POST',
      url: '/tasks/T-0002/workspace',
      headers: authorization,
      payload: { base: 'nope' },
    })
    expect(missing.statusCode).toBe(409)
  })
})

describe('repositories', () => {
  it('lists the default repository, adds another, and makes its tasks work there', async () => {
    const listed = await app.inject({ method: 'GET', url: '/repos', headers: authorization })
    expect(listed.json().repos).toEqual([{ id: 'default', name: 'repo', path: repo.dir }])

    const other = join(repo.root, 'other')
    mkdirSync(other)
    runGit(other, 'init', '-b', 'trunk')
    runGit(other, 'commit', '--allow-empty', '-m', 'first')
    const added = await app.inject({
      method: 'POST',
      url: '/repos',
      headers: authorization,
      payload: { path: other, name: 'Other app' },
    })
    expect(added.statusCode).toBe(201)
    expect(added.json().repo).toMatchObject({ id: 'other-app', name: 'Other app' })
    expect(realpathSync.native(added.json().repo.path)).toBe(realpathSync.native(other))

    const branches = await app.inject({
      method: 'GET',
      url: '/git/branches?repo=other-app',
      headers: authorization,
    })
    expect(branches.json()).toEqual({ branches: ['trunk'], current: 'trunk' })

    const created = await app.inject({
      method: 'POST',
      url: '/tasks',
      headers: authorization,
      payload: { title: 'Fix it there', repo: 'other-app' },
    })
    expect(created.json().task.repo).toBe('other-app')
    const workspace = await app.inject({
      method: 'POST',
      url: '/tasks/T-0001/workspace',
      headers: authorization,
    })
    const folder: string = workspace.json().task.workspace
    expect(runGit(folder, 'log', '-1', '--format=%s').trim()).toBe('first')
  })

  it('refuses an unknown repository, a folder that is not a repository, and a repeat', async () => {
    const unknown = await app.inject({
      method: 'POST',
      url: '/tasks',
      headers: authorization,
      payload: { title: 'x', repo: 'nope' },
    })
    expect(unknown.statusCode).toBe(404)

    const plain = join(repo.root, 'plain')
    mkdirSync(plain)
    const notGit = await app.inject({
      method: 'POST',
      url: '/repos',
      headers: authorization,
      payload: { path: plain },
    })
    expect(notGit.statusCode).toBe(409)

    const again = await app.inject({
      method: 'POST',
      url: '/repos',
      headers: authorization,
      payload: { path: repo.dir },
    })
    expect(again.statusCode).toBe(400)

    const relative = await app.inject({
      method: 'POST',
      url: '/repos',
      headers: authorization,
      payload: { path: 'some/where' },
    })
    expect(relative.statusCode).toBe(400)
  })

  it('removes a repository once it has no workspaces, and keeps its tasks', async () => {
    const other = join(repo.root, 'other')
    mkdirSync(other)
    runGit(other, 'init', '-b', 'trunk')
    runGit(other, 'commit', '--allow-empty', '-m', 'first')
    const settingsText = '{ "stages": ["implement"] }\n'
    writeFileSync(join(other, '.natamux.json'), settingsText)
    await app.inject({
      method: 'POST',
      url: '/repos',
      headers: authorization,
      payload: { path: other, name: 'Other app' },
    })
    for (const title of ['With a workspace', 'Without one']) {
      await app.inject({
        method: 'POST',
        url: '/tasks',
        headers: authorization,
        payload: { title, repo: 'other-app' },
      })
    }
    await app.inject({ method: 'POST', url: '/tasks/T-0001/workspace', headers: authorization })

    const busy = await app.inject({
      method: 'DELETE',
      url: '/repos/other-app',
      headers: authorization,
    })
    expect(busy.statusCode).toBe(409)
    expect(busy.json()).toMatchObject({
      error: 'conflict',
      message: 'Remove the 1 workspace of other-app first',
    })

    await app.inject({ method: 'DELETE', url: '/tasks/T-0001/workspace', headers: authorization })
    const taskFile = join(repo.root, 'data', 'tasks', 'T-0002.md')
    const taskText = readFileSync(taskFile, 'utf8')
    const removed = await app.inject({
      method: 'DELETE',
      url: '/repos/other-app',
      headers: authorization,
    })
    expect(removed.statusCode).toBe(204)
    expect(removed.body).toBe('')
    const listed = await app.inject({ method: 'GET', url: '/repos', headers: authorization })
    expect(listed.json().repos.map((entry: { id: string }) => entry.id)).toEqual(['default'])
    expect(existsSync(other)).toBe(true)
    expect(readFileSync(join(other, '.natamux.json'), 'utf8')).toBe(settingsText)
    expect(runGit(other, 'branch', '--list', 'natamux/*')).toContain('natamux/t-0001')
    expect(readFileSync(taskFile, 'utf8')).toBe(taskText)

    const kept = await app.inject({ method: 'GET', url: '/tasks/T-0002', headers: authorization })
    expect(kept.json().task.repo).toBe('other-app')
    const workspace = await app.inject({
      method: 'POST',
      url: '/tasks/T-0002/workspace',
      headers: authorization,
    })
    expect(workspace.statusCode).toBe(404)
    expect(workspace.json().message).toBe('Repository other-app is not known. Add it first')
  })

  it('removes the default repository like any other', async () => {
    const remove = (id: string) =>
      app.inject({ method: 'DELETE', url: `/repos/${id}`, headers: authorization })
    await app.inject({
      method: 'POST',
      url: '/workspaces',
      headers: authorization,
      payload: { title: 'In the default repository' },
    })

    const busy = await remove('default')
    expect(busy.statusCode).toBe(409)
    expect(busy.json().message).toBe('Remove the 1 workspace of default first')

    await app.inject({ method: 'DELETE', url: '/tasks/T-0001/workspace', headers: authorization })
    const removed = await remove('default')
    expect(removed.statusCode).toBe(204)
    const listed = await app.inject({ method: 'GET', url: '/repos', headers: authorization })
    expect(listed.json().repos).toEqual([])
    expect(existsSync(repo.dir)).toBe(true)

    const again = await remove('default')
    expect(again.statusCode).toBe(404)
    expect(again.json().error).toBe('not_found')
    const unknown = await remove('missing')
    expect(unknown.statusCode).toBe(404)
    expect(unknown.json().error).toBe('not_found')
    expect((await remove('Bad_Id')).statusCode).toBe(400)
  })

  it('asks for a project first when a task names none and there is no default one', async () => {
    await app.inject({ method: 'DELETE', url: '/repos/default', headers: authorization })

    const task = await app.inject({
      method: 'POST',
      url: '/tasks',
      headers: authorization,
      payload: { title: 'Nowhere to go' },
    })
    expect(task.statusCode).toBe(404)
    expect(task.json().message).toMatch(/Add a project first/)

    const workspace = await app.inject({
      method: 'POST',
      url: '/workspaces',
      headers: authorization,
      payload: { title: 'Nowhere to go' },
    })
    expect(workspace.statusCode).toBe(404)
    expect(workspace.json().message).toMatch(/Add a project first/)

    for (const url of ['/tasks', '/workspaces']) {
      const explicit = await app.inject({
        method: 'POST',
        url,
        headers: authorization,
        payload: { title: 'Nowhere to go', repo: null },
      })
      expect(explicit.statusCode).toBe(404)
      expect(explicit.json().message).toMatch(/Add a project first/)
    }

    const listed = await app.inject({ method: 'GET', url: '/tasks', headers: authorization })
    expect(listed.json().tasks).toEqual([])
    expect(existsSync(join(repo.root, 'worktrees'))).toBe(false)
    const repos = await app.inject({ method: 'GET', url: '/repos', headers: authorization })
    expect(repos.statusCode).toBe(200)
  })

  it('puts a task that names no repository, or null, in the default one', async () => {
    for (const payload of [{ title: 'Missing' }, { title: 'Null', repo: null }]) {
      const created = await app.inject({
        method: 'POST',
        url: '/tasks',
        headers: authorization,
        payload,
      })
      expect(created.statusCode).toBe(201)
      expect(created.json().task.repo).toBeNull()
    }
    const workspace = await app.inject({
      method: 'POST',
      url: '/workspaces',
      headers: authorization,
      payload: { title: 'Null workspace', repo: null },
    })
    expect(workspace.statusCode).toBe(201)
    expect(workspace.json().task.workspace).toContain('T-0003')
  })

  it('tells the dashboards when a repository is added or removed', async () => {
    const messages: unknown[] = []
    bus.subscribe((message) => messages.push(message))
    const other = join(repo.root, 'other')
    mkdirSync(other)
    runGit(other, 'init', '-b', 'trunk')

    await app.inject({
      method: 'POST',
      url: '/repos',
      headers: authorization,
      payload: { path: other },
    })
    await app.inject({ method: 'DELETE', url: '/repos/other', headers: authorization })
    await app.inject({ method: 'DELETE', url: '/repos/missing', headers: authorization })

    expect(messages).toEqual([{ kind: 'repos.changed' }, { kind: 'repos.changed' }])
  })
})

describe('project settings', () => {
  const settingsUrl = '/repos/default/settings'

  it('saves the permission mode of the agents, and refuses an unknown one', async () => {
    const before = await app.inject({ method: 'GET', url: settingsUrl, headers: authorization })
    expect(before.json().settings.agents).toEqual({ permissionMode: 'auto' })

    const saved = await app.inject({
      method: 'PUT',
      url: settingsUrl,
      headers: authorization,
      payload: { agents: { permissionMode: 'ask' } },
    })
    expect(saved.statusCode).toBe(200)
    const after = await app.inject({ method: 'GET', url: settingsUrl, headers: authorization })
    expect(after.json().settings.agents).toEqual({ permissionMode: 'ask' })

    const wrong = await app.inject({
      method: 'PUT',
      url: settingsUrl,
      headers: authorization,
      payload: { agents: { permissionMode: 'yolo' } },
    })
    expect(wrong.statusCode).toBe(400)
    expect(wrong.json().message).toContain('agents.permissionMode')
  })
})
