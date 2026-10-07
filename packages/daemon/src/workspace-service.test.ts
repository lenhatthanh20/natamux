import { existsSync, mkdirSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { type DaemonMessage, EventBus } from './event-bus.js'
import { GitError } from './git.js'
import { createTestRepo, runGit, type TestRepo } from './git-repo.fixture.js'
import { TaskIndex } from './task-index.js'
import { TaskNotFoundError, TaskService } from './task-service.js'
import {
  branchNameFor,
  slugify,
  WorkspaceConflictError,
  WorkspaceService,
} from './workspace-service.js'

let repo: TestRepo
let index: TaskIndex
let messages: DaemonMessage[]
let tasks: TaskService
let workspaces: WorkspaceService

beforeEach(() => {
  repo = createTestRepo()
  const dataDir = join(repo.root, 'data')
  mkdirSync(dataDir)
  index = new TaskIndex(':memory:')
  messages = []
  const bus = new EventBus()
  bus.subscribe((message) => messages.push(message))
  tasks = new TaskService({ dataDir, index, bus })
  workspaces = new WorkspaceService({
    tasks,
    repoDir: repo.dir,
    worktreesDir: join(repo.root, 'worktrees'),
  })
})

afterEach(() => {
  index.close()
  repo.cleanup()
})

function branchExists(branch: string): boolean {
  return runGit(repo.dir, 'branch', '--list', branch).trim() !== ''
}

describe('slugify and branchNameFor', () => {
  it.each([
    ['Fix the bug', 'fix-the-bug'],
    ['  Fix: the   BUG!!  ', 'fix-the-bug'],
    ['日本語', 'task'],
    ['---', 'task'],
    ['a'.repeat(60), 'a'.repeat(40)],
    ['word '.repeat(9), 'word-word-word-word-word-word-word-word'],
  ])('turns %j into %j', (title, slug) => {
    expect(slugify(title)).toBe(slug)
  })

  it('builds a branch name from the task id and title', () => {
    expect(branchNameFor({ id: 'T-0042', title: 'Fix the bug' })).toBe('helmsman/t-0042-fix-the-bug')
  })
})

describe('create', () => {
  it('adds a worktree and records it on the task', async () => {
    const task = tasks.createTask({ title: 'Fix the bug' })

    const updated = await workspaces.create(task.id)

    expect(updated.workspace).toBe(join(repo.root, 'worktrees', 'T-0001'))
    expect(updated.branch).toBe('helmsman/t-0001-fix-the-bug')
    expect(existsSync(join(updated.workspace ?? '', '.git'))).toBe(true)
    expect(branchExists('helmsman/t-0001-fix-the-bug')).toBe(true)
    expect(tasks.getTask(task.id)).toEqual(updated)
  })

  it('logs a workspace.created event and publishes the change', async () => {
    const task = tasks.createTask({ title: 'Fix the bug' })

    await workspaces.create(task.id)

    expect(tasks.listEvents(task.id).map((entry) => entry.event.type)).toEqual([
      'task.created',
      'workspace.created',
    ])
    expect(messages.at(-1)).toMatchObject({ kind: 'task.updated', accepted: true })
  })

  it('does not change the state of the task', async () => {
    const task = tasks.createTask({ title: 'Fix the bug' })
    const updated = await workspaces.create(task.id)
    expect(updated.state).toBe('draft')
  })

  it('refuses a second workspace for the same task', async () => {
    const task = tasks.createTask({ title: 'Fix the bug' })
    await workspaces.create(task.id)

    await expect(workspaces.create(task.id)).rejects.toThrow(WorkspaceConflictError)
  })

  it('refuses a task that is done or cancelled', async () => {
    const task = tasks.createTask({ title: 'Fix the bug' })
    tasks.applyEvent(task.id, { source: 'human', type: 'task.cancelled' })

    await expect(workspaces.create(task.id)).rejects.toThrow(/cancelled/)
  })

  it('throws TaskNotFoundError for an unknown task', async () => {
    await expect(workspaces.create('T-0099')).rejects.toThrow(TaskNotFoundError)
  })

  it('lets only one of two requests at the same time through', async () => {
    const task = tasks.createTask({ title: 'Fix the bug' })

    const results = await Promise.allSettled([workspaces.create(task.id), workspaces.create(task.id)])

    expect(results.filter((result) => result.status === 'fulfilled')).toHaveLength(1)
    const failure = results.find((result) => result.status === 'rejected')
    expect(failure?.status === 'rejected' && failure.reason).toBeInstanceOf(WorkspaceConflictError)
  })

  it('leaves the task as it was when git fails', async () => {
    const task = tasks.createTask({ title: 'Fix the bug' })
    runGit(repo.dir, 'branch', 'helmsman/t-0001-fix-the-bug')

    await expect(workspaces.create(task.id)).rejects.toThrow(GitError)

    expect(tasks.getTask(task.id).workspace).toBeNull()
  })
})

describe('remove', () => {
  it('removes the worktree, and the branch when it is merged', async () => {
    const task = tasks.createTask({ title: 'Fix the bug' })
    const created = await workspaces.create(task.id)

    const result = await workspaces.remove(task.id, { deleteBranch: true })

    expect(result.branchDeleted).toBe(true)
    expect(result.task.workspace).toBeNull()
    expect(result.task.branch).toBeNull()
    expect(existsSync(created.workspace ?? '')).toBe(false)
    expect(branchExists('helmsman/t-0001-fix-the-bug')).toBe(false)
    expect(tasks.listEvents(task.id).at(-1)?.event.type).toBe('workspace.removed')
  })

  it('keeps the branch when asked to', async () => {
    const task = tasks.createTask({ title: 'Fix the bug' })
    await workspaces.create(task.id)

    const result = await workspaces.remove(task.id, { deleteBranch: false })

    expect(result.branchDeleted).toBe(false)
    expect(result.task.branch).toBe('helmsman/t-0001-fix-the-bug')
    expect(branchExists('helmsman/t-0001-fix-the-bug')).toBe(true)
  })

  it('keeps a branch that has commits that are not merged', async () => {
    const task = tasks.createTask({ title: 'Fix the bug' })
    const created = await workspaces.create(task.id)
    runGit(created.workspace ?? '', 'commit', '--allow-empty', '-m', 'work')

    const result = await workspaces.remove(task.id, { deleteBranch: true })

    expect(result.branchDeleted).toBe(false)
    expect(result.task.workspace).toBeNull()
    expect(result.task.branch).toBe('helmsman/t-0001-fix-the-bug')
    expect(branchExists('helmsman/t-0001-fix-the-bug')).toBe(true)
  })

  it('refuses to remove a worktree with files that are not committed', async () => {
    const task = tasks.createTask({ title: 'Fix the bug' })
    const created = await workspaces.create(task.id)
    writeFileSync(join(created.workspace ?? '', 'notes.txt'), 'work in progress')

    await expect(workspaces.remove(task.id, { deleteBranch: true })).rejects.toThrow(GitError)

    expect(tasks.getTask(task.id).workspace).toBe(created.workspace)
    expect(existsSync(join(created.workspace ?? '', 'notes.txt'))).toBe(true)
  })

  it('refuses a task that has no workspace', async () => {
    const task = tasks.createTask({ title: 'Fix the bug' })

    await expect(workspaces.remove(task.id, { deleteBranch: true })).rejects.toThrow(
      WorkspaceConflictError,
    )
  })

  it('lets the task get a new workspace after the old one is removed', async () => {
    const task = tasks.createTask({ title: 'Fix the bug' })
    await workspaces.create(task.id)
    await workspaces.remove(task.id, { deleteBranch: true })

    const again = await workspaces.create(task.id)

    expect(again.workspace).not.toBeNull()
  })
})
