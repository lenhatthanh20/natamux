import { mkdirSync, readFileSync } from 'node:fs'
import { join } from 'node:path'
import { DEFAULT_PROJECT_SETTINGS } from '@natamux/core'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { AgentRunner } from '../agent-runner.js'
import { decodeAttachments } from '../attachments.js'
import { EventBus } from '../event-bus.js'
import { newWorkspace } from '../new-workspace.js'
import { StageRunner } from '../stage-runner.js'
import { TaskIndex } from '../task-index.js'
import { type SettingsOf, TaskService } from '../task-service.js'
import { TerminalService } from '../terminal-service.js'
import { WorkspaceConflictError, WorkspaceService } from '../workspace-service.js'
import { createTestRepo, type TestRepo } from './git-repo.fixture.js'
import { type FakePty, fakeSpawner } from './terminal-fake.fixture.js'

let repo: TestRepo
let index: TaskIndex
let tasks: TaskService
let ptys: FakePty[]
let parts: { tasks: TaskService; workspaces: WorkspaceService; stages: StageRunner }

beforeEach(() => {
  repo = createTestRepo()
  mkdirSync(join(repo.root, 'data'))
  index = new TaskIndex(':memory:')
  build()
})

// Without settingsOf, tasks go through the legacy pipeline.
function build(settingsOf?: SettingsOf) {
  const dataDir = join(repo.root, 'data')
  const bus = new EventBus()
  tasks = new TaskService({
    dataDir,
    index,
    bus,
    ...(settingsOf === undefined ? {} : { settingsOf }),
  })
  const workspaces = new WorkspaceService({
    dataDir,
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
  const stages = new StageRunner({
    tasks,
    terminals,
    agents,
    bus,
    dataDir,
    repoPath: () => repo.dir,
  })
  stages.start()
  parts = { tasks, workspaces, stages }
}

afterEach(async () => {
  // Background git work in the worktree would keep its folder busy on Windows.
  await parts.stages.settle()
  parts.stages.stop()
  index.close()
  repo.cleanup()
})

describe('newWorkspace', () => {
  it('makes a task with a worktree and leaves the shell to the user', async () => {
    const task = await newWorkspace(parts, { title: 'Fix the login page' })

    expect(task).toMatchObject({ title: 'Fix the login page', state: 'draft' })
    expect(task.workspace).not.toBeNull()
    expect(task.branch).toBe('natamux/t-0001-fix-the-login-page')
    expect(ptys).toHaveLength(0)
  })

  it('keeps the notes of a task with no agent', async () => {
    const task = await newWorkspace(parts, {
      title: 'Fix the login page',
      prompt: 'Keep the old URL.\nAsk before a redirect.',
    })

    expect(task.description).toBe('Keep the old URL.\nAsk before a redirect.')
    expect(tasks.getTask(task.id).description).toBe('Keep the old URL.\nAsk before a redirect.')
    expect(ptys).toHaveLength(0)
  })

  it('starts the first stage with an agent, and the prompt becomes the notes', async () => {
    const task = await newWorkspace(parts, {
      title: 'Speed up search',
      agent: 'claude-code',
      prompt: 'Keep it under 100 ms',
    })

    expect(task.agent).toBe('claude-code')
    expect(task.description).toBe('Keep it under 100 ms')
    expect(task).toMatchObject({ stage: 'implement', state: 'running' })
    expect(ptys).toHaveLength(1)
  })

  it('starts no agent with the default pipeline, until a person starts one', async () => {
    parts.stages.stop()
    build(() => DEFAULT_PROJECT_SETTINGS)

    const task = await newWorkspace(parts, { title: 'Fix the login page' })
    await parts.stages.settle()

    expect(tasks.getTask(task.id)).toMatchObject({ stage: 'spec', state: 'ready' })
    expect(ptys).toHaveLength(0)

    await parts.stages.startByHand(task.id, 'claude-code')

    expect(tasks.getTask(task.id)).toMatchObject({ stage: 'spec', state: 'running' })
    expect(ptys).toHaveLength(1)
  })

  it('still starts no agent after the daemon restarts', async () => {
    parts.stages.stop()
    build(() => DEFAULT_PROJECT_SETTINGS)
    const task = await newWorkspace(parts, { title: 'Fix the login page' })
    await parts.stages.settle()

    parts.stages.stop()
    build(() => DEFAULT_PROJECT_SETTINGS)
    parts.stages.resumeAgents()
    tasks.applyEvent(task.id, {
      source: 'human',
      type: 'state.overridden',
      payload: { to: 'ready', stage: 'spec' },
    })
    await parts.stages.settle()

    expect(tasks.getTask(task.id)).toMatchObject({ withoutAgent: true, state: 'ready' })
    expect(ptys).toHaveLength(0)
  })

  it('puts the attachments into the new worktree and lists them in the context', async () => {
    parts.stages.stop()
    build(() => DEFAULT_PROJECT_SETTINGS)
    const attachments = decodeAttachments([
      { name: 'screenshot.png', data: Buffer.from('image').toString('base64') },
    ])

    const task = await newWorkspace(parts, { title: 'Fix the login page', attachments })
    await parts.stages.settle()

    const folder = join(task.workspace ?? '', '.natamux-task')
    expect(readFileSync(join(folder, 'attachments', 'screenshot.png'), 'utf8')).toBe('image')
    expect(readFileSync(join(folder, 'context.md'), 'utf8')).toContain(
      '- `.natamux-task/attachments/screenshot.png` (5 B)',
    )
  })

  it('refuses a base branch that does not exist and makes no task', async () => {
    await expect(newWorkspace(parts, { title: 'X', base: 'no-such-branch' })).rejects.toThrow(
      WorkspaceConflictError,
    )
    expect(tasks.listTasks()).toHaveLength(0)
  })

  it('cancels the new task when the worktree cannot be made', async () => {
    vi.spyOn(parts.workspaces, 'create').mockRejectedValue(new Error('index.lock exists'))

    await expect(newWorkspace(parts, { title: 'X' })).rejects.toThrow('index.lock exists')
    expect(tasks.listTasks().map((task) => task.state)).toEqual(['cancelled'])
  })
})
