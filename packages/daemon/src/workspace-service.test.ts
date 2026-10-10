import { existsSync, mkdirSync, rmSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { type DaemonMessage, EventBus } from './event-bus.js'
import { GitError, NothingToCommitError, type RunGh } from './git.js'
import { addBareOrigin, createTestRepo, runGit, type TestRepo } from './git-repo.fixture.js'
import { TaskIndex } from './task-index.js'
import { TaskNotFoundError, TaskService } from './task-service.js'
import {
  branchNameFor,
  FileNotChangedError,
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
    expect(branchNameFor({ id: 'T-0042', title: 'Fix the bug' })).toBe('natamux/t-0042-fix-the-bug')
  })
})

describe('create', () => {
  it('adds a worktree and records it on the task', async () => {
    const task = tasks.createTask({ title: 'Fix the bug' })

    const updated = await workspaces.create(task.id)

    expect(updated.workspace).toBe(join(repo.root, 'worktrees', 'T-0001'))
    expect(updated.branch).toBe('natamux/t-0001-fix-the-bug')
    expect(existsSync(join(updated.workspace ?? '', '.git'))).toBe(true)
    expect(branchExists('natamux/t-0001-fix-the-bug')).toBe(true)
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

    const results = await Promise.allSettled([
      workspaces.create(task.id),
      workspaces.create(task.id),
    ])

    expect(results.filter((result) => result.status === 'fulfilled')).toHaveLength(1)
    const failure = results.find((result) => result.status === 'rejected')
    expect(failure?.status === 'rejected' && failure.reason).toBeInstanceOf(WorkspaceConflictError)
  })

  it('leaves the task as it was when git fails', async () => {
    const task = tasks.createTask({ title: 'Fix the bug' })
    runGit(repo.dir, 'branch', 'natamux/t-0001-fix-the-bug')

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
    expect(branchExists('natamux/t-0001-fix-the-bug')).toBe(false)
    expect(tasks.listEvents(task.id).at(-1)?.event.type).toBe('workspace.removed')
  })

  it('keeps the branch when asked to', async () => {
    const task = tasks.createTask({ title: 'Fix the bug' })
    await workspaces.create(task.id)

    const result = await workspaces.remove(task.id, { deleteBranch: false })

    expect(result.branchDeleted).toBe(false)
    expect(result.task.branch).toBe('natamux/t-0001-fix-the-bug')
    expect(branchExists('natamux/t-0001-fix-the-bug')).toBe(true)
  })

  it('keeps a branch that has commits that are not merged', async () => {
    const task = tasks.createTask({ title: 'Fix the bug' })
    const created = await workspaces.create(task.id)
    runGit(created.workspace ?? '', 'commit', '--allow-empty', '-m', 'work')

    const result = await workspaces.remove(task.id, { deleteBranch: true })

    expect(result.branchDeleted).toBe(false)
    expect(result.task.workspace).toBeNull()
    expect(result.task.branch).toBe('natamux/t-0001-fix-the-bug')
    expect(branchExists('natamux/t-0001-fix-the-bug')).toBe(true)
  })

  it('refuses a worktree with changes not committed, before ending its shells', async () => {
    const closed: string[] = []
    const guarded = new WorkspaceService({
      tasks,
      repoDir: repo.dir,
      worktreesDir: join(repo.root, 'worktrees'),
      closeTerminals: (taskId) => closed.push(taskId),
    })
    const task = tasks.createTask({ title: 'Fix the bug' })
    const created = await guarded.create(task.id)
    writeFileSync(join(created.workspace ?? '', 'notes.txt'), 'work in progress')

    await expect(guarded.remove(task.id, { deleteBranch: true })).rejects.toThrow(
      WorkspaceConflictError,
    )

    expect(closed).toEqual([])
    expect(tasks.getTask(task.id).workspace).toBe(created.workspace)
    expect(existsSync(join(created.workspace ?? '', 'notes.txt'))).toBe(true)

    runGit(created.workspace ?? '', 'add', 'notes.txt')
    runGit(created.workspace ?? '', 'commit', '-m', 'Notes')
    await guarded.remove(task.id, { deleteBranch: false })
    expect(closed).toEqual([task.id])
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

  it('goes on with the branch that was kept when the task gets a new workspace', async () => {
    const task = tasks.createTask({ title: 'Fix the bug' })
    const created = await workspaces.create(task.id)
    runGit(created.workspace ?? '', 'commit', '--allow-empty', '-m', 'work')
    await workspaces.remove(task.id, { deleteBranch: true })

    const again = await workspaces.create(task.id)

    expect(again.branch).toBe('natamux/t-0001-fix-the-bug')
    expect(runGit(again.workspace ?? '', 'log', '-1', '--format=%s').trim()).toBe('work')
  })
})

describe('graph', () => {
  it('shows the task branch next to the branch of the main checkout', async () => {
    const task = tasks.createTask({ title: 'Fix the bug' })
    const created = await workspaces.create(task.id)
    runGit(created.workspace ?? '', 'commit', '--allow-empty', '-m', 'work')

    const graph = await workspaces.graph(task.id)

    expect(graph).toMatchObject({ branch: created.branch, base: 'main', ahead: 1, behind: 0 })
    expect(graph.commits.map((commit) => [commit.subject, commit.onBranch])).toEqual([
      ['work', true],
      ['init', false],
    ])
  })

  it('refuses a task that has no workspace', async () => {
    const task = tasks.createTask({ title: 'Fix the bug' })
    await expect(workspaces.graph(task.id)).rejects.toThrow(WorkspaceConflictError)
  })
})

describe('changes and fileDiff', () => {
  async function taskWithWorkspace() {
    const task = tasks.createTask({ title: 'Fix the bug' })
    const created = await workspaces.create(task.id)
    return { task, workspace: created.workspace ?? '' }
  }

  it('lists the files that changed in the workspace', async () => {
    const { task, workspace } = await taskWithWorkspace()
    expect(await workspaces.changes(task.id)).toEqual([])

    writeFileSync(join(workspace, 'notes.txt'), 'hello\n')

    expect(await workspaces.changes(task.id)).toEqual([
      { path: 'notes.txt', status: 'U', originalPath: null, added: 1, removed: 0 },
    ])
  })

  it('gives both sides of a changed file', async () => {
    const { task, workspace } = await taskWithWorkspace()
    writeFileSync(join(workspace, 'notes.txt'), 'hello\n')

    expect(await workspaces.fileDiff(task.id, 'notes.txt')).toEqual({
      path: 'notes.txt',
      status: 'U',
      original: null,
      modified: 'hello\n',
      binary: false,
      tooLarge: false,
    })
  })

  it.each(['missing.txt', '../repo/.git/config', '/etc/passwd', '..'])(
    'refuses %j, which is not a changed file',
    async (path) => {
      const { task } = await taskWithWorkspace()
      await expect(workspaces.fileDiff(task.id, path)).rejects.toThrow(FileNotChangedError)
    },
  )

  it('refuses a file that was changed and then put back', async () => {
    const { task, workspace } = await taskWithWorkspace()
    writeFileSync(join(workspace, 'notes.txt'), 'hello\n')
    expect((await workspaces.changes(task.id)).length).toBe(1)
    rmSync(join(workspace, 'notes.txt'))

    await expect(workspaces.fileDiff(task.id, 'notes.txt')).rejects.toThrow(FileNotChangedError)
  })

  it('gives one side of a changed image, and refuses an image that did not change', async () => {
    const { task, workspace } = await taskWithWorkspace()
    writeFileSync(join(workspace, 'logo.png'), Buffer.from([0x89, 0x50, 0x4e, 0x47, 0]))

    const image = await workspaces.fileImage(task.id, 'logo.png', 'modified')

    expect(image.type).toBe('image/png')
    expect([...image.bytes]).toEqual([0x89, 0x50, 0x4e, 0x47, 0])
    await expect(workspaces.fileImage(task.id, 'other.png', 'modified')).rejects.toThrow(
      FileNotChangedError,
    )
  })

  it('refuses a task that has no workspace', async () => {
    const task = tasks.createTask({ title: 'Fix the bug' })

    await expect(workspaces.changes(task.id)).rejects.toThrow(WorkspaceConflictError)
    await expect(workspaces.fileDiff(task.id, 'a.txt')).rejects.toThrow(WorkspaceConflictError)
  })

  it('throws TaskNotFoundError for an unknown task', async () => {
    await expect(workspaces.changes('T-0099')).rejects.toThrow(TaskNotFoundError)
  })
})

describe('commit, push and pull request', () => {
  const PR_URL = 'https://github.com/owner/repo/pull/7'
  let ghCalls: (readonly string[])[]

  beforeEach(() => {
    ghCalls = []
    const gh: RunGh = async (_cwd, args) => {
      ghCalls.push(args)
      return `${PR_URL}\n`
    }
    workspaces = new WorkspaceService({
      tasks,
      repoDir: repo.dir,
      worktreesDir: join(repo.root, 'worktrees'),
      gh,
    })
  })

  async function taskWithWork() {
    const task = tasks.createTask({ title: 'Fix the bug', description: 'The details.' })
    const created = await workspaces.create(task.id)
    writeFileSync(join(created.workspace ?? '', 'fix.txt'), 'fixed\n')
    return created
  }

  it('commits the changes of the workspace and logs the commit', async () => {
    const task = await taskWithWork()

    const { hash } = await workspaces.commit(task.id, 'Fix the bug')

    expect(await workspaces.changes(task.id)).toEqual([])
    expect(runGit(task.workspace ?? '', 'rev-parse', 'HEAD').trim()).toBe(hash)
    expect(tasks.listEvents(task.id).at(-1)?.event).toMatchObject({
      type: 'git.committed',
      payload: { hash, subject: 'Fix the bug' },
    })
    await expect(workspaces.commit(task.id, 'Again')).rejects.toThrow(NothingToCommitError)
  })

  it('pushes the task branch to origin', async () => {
    const remote = addBareOrigin(repo)
    const task = await taskWithWork()
    await workspaces.commit(task.id, 'Fix the bug')

    const pushed = await workspaces.push(task.id)

    expect(runGit(remote, 'branch', '--list', task.branch ?? '').trim()).not.toBe('')
    expect(tasks.listEvents(task.id).at(-1)?.event).toMatchObject({
      type: 'git.pushed',
      payload: { branch: task.branch },
    })
    expect(pushed.pr).toBeNull()
  })

  it('pushes, then opens a pull request and keeps its address', async () => {
    addBareOrigin(repo)
    const task = await taskWithWork()
    await workspaces.commit(task.id, 'Fix the bug')

    const updated = await workspaces.openPullRequest(task.id)

    expect(updated.pr).toBe(PR_URL)
    expect(ghCalls).toEqual([
      ['pr', 'create', '--head', task.branch, '--title', 'Fix the bug', '--body', 'The details.'],
    ])
    expect(
      tasks
        .listEvents(task.id)
        .map((entry) => entry.event.type)
        .slice(-2),
    ).toEqual(['git.pushed', 'pr.opened'])
    await expect(workspaces.openPullRequest(task.id)).rejects.toThrow(WorkspaceConflictError)
  })

  it('does not ask gh when the push fails', async () => {
    const task = await taskWithWork()
    await expect(workspaces.openPullRequest(task.id)).rejects.toThrow(GitError)
    expect(ghCalls).toEqual([])
    expect(tasks.getTask(task.id).pr).toBeNull()
  })

  it('refuses a task without a workspace', async () => {
    const task = tasks.createTask({ title: 'Fix the bug' })
    await expect(workspaces.commit(task.id, 'x')).rejects.toThrow(WorkspaceConflictError)
    await expect(workspaces.push(task.id)).rejects.toThrow(WorkspaceConflictError)
  })
})

describe('diffStats', () => {
  it('gives the size of the work of each task with a workspace', async () => {
    const task = tasks.createTask({ title: 'Fix the bug' })
    tasks.createTask({ title: 'No workspace' })
    const created = await workspaces.create(task.id)
    writeFileSync(join(created.workspace ?? '', 'notes.txt'), 'a\nb\n')
    runGit(created.workspace ?? '', 'add', 'notes.txt')

    expect(await workspaces.diffStats()).toEqual({
      [task.id]: { files: 1, added: 2, removed: 0 },
    })
  })
})

describe('onCreated', () => {
  it('is told about a new workspace, and its failure does not undo the workspace', async () => {
    const seen: string[] = []
    const watched = new WorkspaceService({
      tasks,
      repoDir: repo.dir,
      worktreesDir: join(repo.root, 'worktrees'),
      onCreated: (task) => {
        seen.push(task.workspace ?? '')
        throw new Error('setup failed')
      },
    })
    const task = tasks.createTask({ title: 'Fix the bug' })

    const created = await watched.create(task.id)

    expect(seen).toEqual([created.workspace])
    expect(existsSync(created.workspace ?? '')).toBe(true)
  })
})

describe('base branch', () => {
  it('lists the local branches and the current one', async () => {
    runGit(repo.dir, 'branch', 'release')
    expect(await workspaces.branches()).toEqual({ branches: ['main', 'release'], current: 'main' })
  })

  it('starts a workspace from the branch it is given', async () => {
    runGit(repo.dir, 'checkout', '-b', 'release')
    runGit(repo.dir, 'commit', '--allow-empty', '-m', 'on release')
    runGit(repo.dir, 'checkout', 'main')
    const task = tasks.createTask({ title: 'Fix the bug' })

    const created = await workspaces.create(task.id, 'release')

    expect(runGit(created.workspace ?? '', 'log', '-1', '--format=%s').trim()).toBe('on release')
  })

  it('refuses a branch that does not exist', async () => {
    const task = tasks.createTask({ title: 'Fix the bug' })
    await expect(workspaces.create(task.id, 'nope')).rejects.toThrow(WorkspaceConflictError)
  })
})
