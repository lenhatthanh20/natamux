import { appendFileSync, existsSync, mkdirSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { monitorEventLoopDelay, performance } from 'node:perf_hooks'
import { ACK_EVERY_BYTES, TASK_ID_HEADER } from '@natamux/core'
import type { FastifyInstance } from 'fastify'
import { afterAll, afterEach, describe, expect, it, vi } from 'vitest'
import { AgentRunner } from '../agent-runner.js'
import { EventBus } from '../event-bus.js'
import { type LoggedEvent, writeEventLog } from '../event-log.js'
import { gitSpawnCount } from '../git.js'
import { IssueImporter } from '../issue-importer.js'
import { IssueLinker } from '../issue-linker.js'
import { ProjectSettingsService } from '../project-settings.js'
import { PullRequestWatcher } from '../pull-request-watcher.js'
import { RepoService } from '../repo-service.js'
import { buildServer } from '../server.js'
import { StageRunner } from '../stage-runner.js'
import { taskFilePath } from '../task-files.js'
import { TaskIndex } from '../task-index.js'
import { TaskService } from '../task-service.js'
import { TemplateService } from '../template-service.js'
import { MAIN_TERMINAL, type TerminalClient, TerminalService } from '../terminal-service.js'
import { WorkspaceService } from '../workspace-service.js'
import { createTestRepo, type TestRepo } from './git-repo.fixture.js'
import { type FakePty, fakeSpawner } from './terminal-fake.fixture.js'

// Numbers for the hot path of the daemon (a hook, a task list, the start), recorded so each PR can
// show its effect on the same cases. They are not limits. It runs only with NATAMUX_BENCH=1, which
// the CI build job sets: every other test run stays fast.
const BENCH_TEST_MS = 600_000
const HOT_TASKS = 200
const HOT_EVENTS = 100
const HOOKS = 5_000
// One list after this many hooks gives 500 lists.
const HOOKS_PER_LIST = 10
const INIT_TASKS = 500
const INIT_EVENTS = 200
const FLOOD_PIECE_CHARS = 4096
const FLOOD_PIECES_PER_TURN = 8
const TOKEN = 'bench-token'

const results: Record<string, unknown>[] = []
const cleanups: (() => Promise<void> | void)[] = []

function round(value: number, digits = 1): number {
  const factor = 10 ** digits
  return Math.round(value * factor) / factor
}

// The nearest-rank percentile of the values.
function percentile(values: number[], p: number): number {
  const sorted = [...values].sort((a, b) => a - b)
  const rank = Math.ceil((p / 100) * sorted.length)
  return sorted[Math.min(sorted.length, Math.max(1, rank)) - 1] ?? 0
}

function nextTurn(): Promise<void> {
  return new Promise((resolve) => setImmediate(resolve))
}

function seedEvents(taskId: string, count: number): LoggedEvent[] {
  const start = Date.parse('2026-01-01T00:00:00.000Z')
  return Array.from({ length: count }, (_, index) => ({
    event: {
      taskId,
      source: 'agent' as const,
      type: 'agent.active' as const,
      at: new Date(start + index * 1000).toISOString(),
      payload: {},
    },
    accepted: true,
  }))
}

// Tasks with their files and event logs, written the way the daemon writes them. The logs are
// written whole (not one event at a time), so seeding 100,000 events stays quick.
function seedTasks(service: TaskService, dataDir: string, tasks: number, events: number): string[] {
  const ids: string[] = []
  for (let index = 0; index < tasks; index += 1) {
    const task = service.createTask({ title: `Bench task ${index + 1}` })
    writeEventLog(dataDir, task.id, seedEvents(task.id, events))
    ids.push(task.id)
  }
  return ids
}

// Acks as the dashboard does, so flow control never puts the shell behind.
function ackingClient(service: TerminalService, taskId: string): TerminalClient {
  let offset = 0
  let acked = 0
  let live = false
  const client: TerminalClient = {
    send: (frame) => {
      if (!(frame instanceof Uint8Array)) {
        if (frame.type === 'replayed') {
          live = true
          offset = frame.offset
          acked = offset
        }
        return
      }
      if (!live) {
        return
      }
      offset += frame.byteLength
      if (offset - acked >= ACK_EVERY_BYTES) {
        const at = offset
        acked = at
        queueMicrotask(() => service.ack(taskId, at, MAIN_TERMINAL, client))
      }
    },
  }
  return client
}

// Prints lines into the shell in turns, until stopped, so the daemon parses a flood while it
// answers the requests.
function startFlood(pty: FakePty) {
  let running = true
  const piece = `${'x'.repeat(FLOOD_PIECE_CHARS - 2)}\r\n`
  const done = (async () => {
    while (running) {
      for (let count = 0; count < FLOOD_PIECES_PER_TURN; count += 1) {
        pty.emit(piece)
      }
      await nextTurn()
    }
  })()
  return async () => {
    running = false
    await done
  }
}

async function startServer() {
  const repo: TestRepo = createTestRepo()
  const dataDir = join(repo.root, 'data')
  mkdirSync(dataDir)
  const webIndexPath = join(repo.root, 'index.html')
  writeFileSync(webIndexPath, '<!doctype html><div id="root"></div>')
  const index = new TaskIndex(join(dataDir, 'index.sqlite'))
  const bus = new EventBus()
  const service = new TaskService({ dataDir, index, bus })
  const repos = new RepoService({
    dataDir,
    defaultPath: repo.dir,
    tasks: () => service.listTasks(),
  })
  const spawner = fakeSpawner()
  const terminals = new TerminalService({
    spawn: spawner.spawn,
    workspaceOf: (taskId) => `/work/${taskId}`,
    env: { PATH: '/bin' },
  })
  const workspaces = new WorkspaceService({
    dataDir,
    tasks: service,
    repoDir: repo.dir,
    worktreesDir: join(repo.root, 'worktrees'),
    repos,
    closeTerminals: (taskId) => terminals.close(taskId),
    runAgentOnce: async () => 'Bench\n',
  })
  const agents = new AgentRunner({
    tasks: service,
    terminals,
    claudeSettingsPath: '/data/claude-code-settings.json',
    platform: 'linux',
    launchDir: dataDir,
  })
  const app: FastifyInstance = await buildServer({
    service,
    workspaces,
    terminals,
    agents,
    pullRequests: new PullRequestWatcher({
      tasks: service,
      cwdOf: () => repo.dir,
      gh: async () => JSON.stringify({ state: 'OPEN', isDraft: false, statusCheckRollup: [] }),
    }),
    issues: new IssueImporter({ tasks: service, repoPath: () => repo.dir, gh: async () => '[]' }),
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
      home: join(repo.root, 'home'),
    }),
    settings: new ProjectSettingsService({ repos }),
    issueLinker: new IssueLinker({ tasks: service, bus, dataDir, repoPath: () => repo.dir }),
    dataDir,
    token: TOKEN,
    agentToken: 'agent-token',
    webIndexPath,
    webDir: repo.root,
    skillsHome: join(repo.root, 'home'),
  })
  cleanups.push(async () => {
    terminals.closeAll()
    await app.close()
    service.close()
    index.close()
    repo.cleanup()
  })
  const ids = seedTasks(service, dataDir, HOT_TASKS, HOT_EVENTS)
  // Syncs the index with the logs, as the start of the daemon does.
  service.init()
  return { app, service, terminals, ptys: spawner.ptys, ids, repo }
}

// What an agent sends in one turn: the prompt, eight tools, the stop.
function hookBody(step: number): Record<string, unknown> {
  const phase = step % 10
  if (phase === 0) {
    return { hook_event_name: 'UserPromptSubmit' }
  }
  if (phase === 9) {
    return { hook_event_name: 'Stop' }
  }
  return {
    hook_event_name: 'PreToolUse',
    tool_name: 'Edit',
    tool_input: { file_path: `src/file-${phase}.ts` },
  }
}

async function measureRun(
  label: string,
  app: FastifyInstance,
  ids: string[],
): Promise<Record<string, unknown>> {
  const headers = { authorization: `Bearer ${TOKEN}` }
  const hookMs: number[] = []
  const listMs: number[] = []
  const spawnsBefore = gitSpawnCount()
  const delay = monitorEventLoopDelay({ resolution: 1 })
  delay.enable()
  const began = performance.now()
  for (let count = 0; count < HOOKS; count += 1) {
    const taskId = ids[count % ids.length] as string
    const at = performance.now()
    const response = await app.inject({
      method: 'POST',
      url: '/hooks/claude-code',
      headers: { ...headers, [TASK_ID_HEADER]: taskId },
      payload: hookBody(Math.floor(count / ids.length)),
    })
    hookMs.push(performance.now() - at)
    expect(response.statusCode).toBe(200)
    // The output of a flooding shell never reaches the answer of a hook.
    expect(response.body).toBe('{}')
    if ((count + 1) % HOOKS_PER_LIST === 0) {
      const listAt = performance.now()
      const list = await app.inject({ method: 'GET', url: '/tasks', headers })
      listMs.push(performance.now() - listAt)
      expect(list.statusCode).toBe(200)
    }
  }
  const seconds = (performance.now() - began) / 1000
  delay.disable()
  expect(listMs).toHaveLength(HOOKS / HOOKS_PER_LIST)
  return {
    measure: label,
    hooks: HOOKS,
    hookP50Ms: round(percentile(hookMs, 50), 2),
    hookP99Ms: round(percentile(hookMs, 99), 2),
    lists: listMs.length,
    listP50Ms: round(percentile(listMs, 50), 2),
    listP99Ms: round(percentile(listMs, 99), 2),
    loopDelayP99Ms: round(delay.percentile(99) / 1e6),
    loopDelayMaxMs: round(delay.max / 1e6),
    seconds: round(seconds, 1),
    gitSpawns: gitSpawnCount() - spawnsBefore,
  }
}

function report(): void {
  if (results.length === 0) {
    return
  }
  const columns = [...new Set(results.flatMap((row) => Object.keys(row)))]
  const table = [
    `| ${columns.join(' | ')} |`,
    `| ${columns.map(() => '---').join(' | ')} |`,
    ...results.map((row) => `| ${columns.map((name) => String(row[name] ?? '')).join(' | ')} |`),
  ].join('\n')
  console.log(`Daemon hot path benchmark\n${table}`)
  console.log(`DAEMON_BENCH_JSON ${JSON.stringify(results)}`)
  const summary = process.env.GITHUB_STEP_SUMMARY
  if (summary) {
    const title = `### Daemon hot path benchmark (job ${process.env.GITHUB_JOB ?? 'local'}, node ${process.version})`
    appendFileSync(summary, `\n${title}\n\n${table}\n`)
  }
}

describe.runIf(process.env.NATAMUX_BENCH === '1')('daemon benchmark', () => {
  afterAll(report)
  afterEach(async () => {
    for (const cleanup of cleanups.splice(0)) {
      await cleanup()
    }
  })

  it(
    'answers hooks and task lists, with and without a flooding shell',
    async () => {
      const { app, terminals, ptys, ids } = await startServer()

      results.push(await measureRun('hooks+lists, quiet', app, ids))
      expect(results[0]).toEqual({
        measure: 'hooks+lists, quiet',
        hooks: HOOKS,
        hookP50Ms: expect.any(Number),
        hookP99Ms: expect.any(Number),
        lists: HOOKS / HOOKS_PER_LIST,
        listP50Ms: expect.any(Number),
        listP99Ms: expect.any(Number),
        loopDelayP99Ms: expect.any(Number),
        loopDelayMaxMs: expect.any(Number),
        seconds: expect.any(Number),
        gitSpawns: expect.any(Number),
      })

      // The shell of the last task, so the flood does not share a session with the first hooks.
      const floodedId = ids[ids.length - 1] as string
      terminals.attach(floodedId, ackingClient(terminals, floodedId), { cols: 120, rows: 40 })
      const pty = ptys[0]
      if (!pty) {
        throw new Error('no shell was started')
      }
      const stopFlood = startFlood(pty)
      try {
        results.push(await measureRun('hooks+lists, shell flood', app, ids))
      } finally {
        await stopFlood()
      }
      expect(results[1]).toMatchObject({ measure: 'hooks+lists, shell flood', hooks: HOOKS })
    },
    BENCH_TEST_MS,
  )

  it(
    'answers a hook for an unknown task, and leaves no files after the run',
    async () => {
      const { app, repo } = await startServer()
      const response = await app.inject({
        method: 'POST',
        url: '/hooks/claude-code',
        headers: { authorization: `Bearer ${TOKEN}`, [TASK_ID_HEADER]: 'T-9999' },
        payload: hookBody(1),
      })
      expect(response.statusCode).toBe(200)
      expect(response.body).toBe('{}')
      // The seeded data lives under the temp folder, which the cleanup removes whole.
      expect(existsSync(repo.root)).toBe(true)
      for (const cleanup of cleanups.splice(0)) {
        await cleanup()
      }
      expect(existsSync(repo.root)).toBe(false)
    },
    BENCH_TEST_MS,
  )

  it('starts with no tasks at all', () => {
    const repo = createTestRepo()
    cleanups.push(() => repo.cleanup())
    const dataDir = join(repo.root, 'data')
    mkdirSync(dataDir)
    const index = new TaskIndex(join(dataDir, 'index.sqlite'))
    const service = new TaskService({ dataDir, index, bus: new EventBus() })
    service.init()
    expect(service.listTasks()).toEqual([])
    service.close()
    index.close()
  })

  it('starts past a corrupt task file', () => {
    const repo = createTestRepo()
    cleanups.push(() => repo.cleanup())
    const dataDir = join(repo.root, 'data')
    mkdirSync(dataDir)
    const index = new TaskIndex(join(dataDir, 'index.sqlite'))
    const service = new TaskService({ dataDir, index, bus: new EventBus() })
    const task = service.createTask({ title: 'Broken soon' })
    writeFileSync(taskFilePath(dataDir, task.id), '---\ntitle: [unclosed\n---\n')
    const errors = vi.spyOn(console, 'error').mockImplementation(() => undefined)
    const started = new TaskService({ dataDir, index, bus: new EventBus() })
    started.init()
    // The index saw the task before its file broke, so that copy stays.
    expect(started.listTasks().map(({ title }) => title)).toEqual(['Broken soon'])
    expect(errors).toHaveBeenCalledTimes(1)
    errors.mockRestore()
    started.close()
    service.close()
    index.close()
  })

  it(
    'starts with many tasks and events',
    async () => {
      const repo = createTestRepo()
      cleanups.push(() => repo.cleanup())
      const dataDir = join(repo.root, 'data')
      mkdirSync(dataDir)
      const seedIndex = new TaskIndex(join(dataDir, 'seed.sqlite'))
      seedTasks(
        new TaskService({ dataDir, index: seedIndex, bus: new EventBus() }),
        dataDir,
        INIT_TASKS,
        INIT_EVENTS,
      )
      seedIndex.close()

      // A new index file is a start with no cache; the second start finds the cache of the first.
      const path = join(dataDir, 'index.sqlite')
      const times: number[] = []
      for (const label of ['cold index', 'warm index']) {
        const index = new TaskIndex(path)
        const service = new TaskService({ dataDir, index, bus: new EventBus() })
        const began = performance.now()
        service.init()
        times.push(performance.now() - began)
        expect(service.listTasks()).toHaveLength(INIT_TASKS)
        service.close()
        index.close()
        results.push({
          measure: `init ${INIT_TASKS} tasks x ${INIT_EVENTS} events, ${label}`,
          initMs: round(times[times.length - 1] as number),
        })
      }
    },
    BENCH_TEST_MS,
  )
})
