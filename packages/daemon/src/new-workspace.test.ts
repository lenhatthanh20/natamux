import { mkdirSync } from 'node:fs'
import { join } from 'node:path'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { AgentRunner } from './agent-runner.js'
import { EventBus } from './event-bus.js'
import { createTestRepo, type TestRepo } from './git-repo.fixture.js'
import { newWorkspace } from './new-workspace.js'
import { TaskIndex } from './task-index.js'
import { TaskService } from './task-service.js'
import { type FakePty, fakeSpawner } from './terminal-fake.fixture.js'
import { TerminalService } from './terminal-service.js'
import { WorkspaceConflictError, WorkspaceService } from './workspace-service.js'

let repo: TestRepo
let index: TaskIndex
let tasks: TaskService
let ptys: FakePty[]
let parts: { tasks: TaskService; workspaces: WorkspaceService; agents: AgentRunner }

beforeEach(() => {
  repo = createTestRepo()
  const dataDir = join(repo.root, 'data')
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
  parts = { tasks, workspaces, agents }
})

afterEach(() => {
  index.close()
  repo.cleanup()
})

describe('newWorkspace', () => {
  it('makes a ready task with a worktree and leaves the shell to the user', async () => {
    const task = await newWorkspace(parts, { title: 'Fix the login page' })

    expect(task).toMatchObject({ title: 'Fix the login page', state: 'ready' })
    expect(task.workspace).not.toBeNull()
    expect(task.branch).toBe('helmsman/t-0001-fix-the-login-page')
    expect(ptys).toHaveLength(0)
  })

  it('starts the agent with the prompt, which also becomes the notes', async () => {
    const task = await newWorkspace(parts, {
      title: 'Speed up search',
      agent: 'claude-code',
      prompt: 'Keep it under 100 ms',
    })

    expect(task.agent).toBe('claude-code')
    expect(task.description).toBe('Keep it under 100 ms')
    expect(ptys).toHaveLength(1)
  })

  it('refuses a base branch that does not exist and makes no task', async () => {
    await expect(newWorkspace(parts, { title: 'X', base: 'no-such-branch' })).rejects.toThrow(
      WorkspaceConflictError,
    )
    expect(tasks.listTasks()).toHaveLength(0)
  })
})
