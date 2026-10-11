import { mkdirSync } from 'node:fs'
import { join } from 'node:path'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { AgentRunner } from './agent-runner.js'
import { decodeAttachments, listAttachments } from './attachments.js'
import { EventBus } from './event-bus.js'
import { fanOut, tryLabel } from './fan-out.js'
import { createTestRepo, type TestRepo } from './git-repo.fixture.js'
import { TaskIndex } from './task-index.js'
import { TaskService } from './task-service.js'
import { type FakePty, fakeSpawner } from './terminal-fake.fixture.js'
import { TerminalService } from './terminal-service.js'
import { WorkspaceService } from './workspace-service.js'

let repo: TestRepo
let index: TaskIndex
let tasks: TaskService
let ptys: FakePty[]
let parts: Parameters<typeof fanOut>[0]
let dataDir: string

beforeEach(() => {
  repo = createTestRepo()
  dataDir = join(repo.root, 'data')
  mkdirSync(dataDir)
  index = new TaskIndex(':memory:')
  tasks = new TaskService({ dataDir, index, bus: new EventBus() })
  const workspaces = new WorkspaceService({
    tasks,
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
    tasks,
    terminals,
    claudeSettingsPath: '/data/settings.json',
    platform: 'linux',
    launchDir: dataDir,
  })
  // No stage runner here: a start by hand goes straight to the agent.
  const stages = {
    startByHand: async (id: string, agent: string, prompt?: string) =>
      agents.start(id, agent, prompt),
  }
  parts = { tasks, workspaces, stages, dataDir }
})

afterEach(() => {
  index.close()
  repo.cleanup()
})

describe('fanOut', () => {
  it('copies a task, and labels it and its tries alike', async () => {
    const source = tasks.createTask({
      title: 'Speed up search',
      description: 'Under 100 ms',
      priority: 'high',
      labels: ['perf'],
    })

    const tries = await fanOut(parts, source.id, { count: 2, start: false })

    const label = tryLabel(source.id)
    expect(label).toBe('tries-t-0001')
    expect(tries.map((task) => task.title)).toEqual([
      'Speed up search (try 2)',
      'Speed up search (try 3)',
    ])
    expect(tries[0]).toMatchObject({
      description: 'Under 100 ms',
      priority: 'high',
      labels: ['perf', label],
      state: 'draft',
      workspace: null,
    })
    expect(tasks.getTask(source.id).labels).toEqual(['perf', label])
    expect(ptys).toHaveLength(0)
  })

  it('gives each try the attachments of the task', async () => {
    const files = decodeAttachments([
      { name: 'a.png', data: Buffer.from('image').toString('base64') },
      { name: 'b.pdf', data: Buffer.from('pdf').toString('base64') },
    ])
    const source = tasks.createTask({ title: 'Speed up search' }, files)

    const tries = await fanOut(parts, source.id, { count: 2, start: false })

    for (const task of tries) {
      expect(listAttachments(dataDir, task.id)).toEqual(listAttachments(dataDir, source.id))
    }
    expect(listAttachments(dataDir, source.id)).toHaveLength(2)
  })

  it('gives each try a workspace and starts Claude Code in it', async () => {
    const source = tasks.createTask({ title: 'Speed up search' })

    const tries = await fanOut(parts, source.id, { count: 2, start: true })

    for (const task of tries) {
      expect(task.agent).toBe('claude-code')
      expect(task.workspace).not.toBeNull()
    }
    expect(ptys).toHaveLength(2)
    expect(ptys[0]?.written[0]).toContain('agent-launch-')
  })
})
