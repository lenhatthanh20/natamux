import {
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  rmSync,
  statSync,
  writeFileSync,
} from 'node:fs'
import { join } from 'node:path'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import {
  addBareOrigin,
  createTestRepo,
  runGit,
  type TestRepo,
} from '../../__tests__/git-repo.fixture.js'
import type { AgentRun } from '../../git/commit-drafter.js'
import { NothingToCommitError } from '../../git/commit-push.js'
import { GitError, gitSpawnCount } from '../../git/exec.js'
import type { RunGh } from '../../git/gh.js'
import { worktreeState } from '../../git/worktrees.js'
import { RepoService } from '../../projects/repo-service.js'
import { type DaemonMessage, EventBus } from '../../shared/event-bus.js'
import { excludeStageDir } from '../../stages/stage-files.js'
import { archiveDir, STAGE_DIR } from '../../tasks/task-files.js'
import { TaskIndex } from '../../tasks/task-index.js'
import { TaskNotFoundError } from '../../tasks/task-inputs.js'
import { TaskService } from '../../tasks/task-service.js'
import { DIFF_STATS_MAX_AGE_MS } from '../diff-stats.js'
import { DIRTY_BURST, DIRTY_DEBOUNCE_MS, DIRTY_MIN_INTERVAL_MS } from '../dirty-tracker.js'
import { GIT_READ_MS } from '../workspace-changes.js'
import { FileNotChangedError, WorkspaceConflictError } from '../workspace-errors.js'
import { WorkspaceService } from '../workspace-service.js'

let repo: TestRepo
let index: TaskIndex
let messages: DaemonMessage[]
let tasks: TaskService
let workspaces: WorkspaceService
let dataDir: string
// The clock of the git reads that are kept for a moment.
let clock: number

beforeEach(() => {
  clock = 0
  repo = createTestRepo()
  dataDir = join(repo.root, 'data')
  mkdirSync(dataDir)
  index = new TaskIndex(':memory:')
  messages = []
  const bus = new EventBus()
  bus.subscribe((message) => messages.push(message))
  tasks = new TaskService({ dataDir, index, bus })
  workspaces = new WorkspaceService({
    dataDir,
    tasks,
    repoDir: repo.dir,
    worktreesDir: join(repo.root, 'worktrees'),
    now: () => clock,
  })
})

afterEach(() => {
  index.close()
  repo.cleanup()
})

function branchExists(branch: string): boolean {
  return runGit(repo.dir, 'branch', '--list', branch).trim() !== ''
}

// Tests that make, change and remove worktrees several times. Git on Windows is slow at that.
const GIT_TEST_MS = 30_000

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

  it("adds the worktree in the project's own worktree folder", async () => {
    const repos = new RepoService({ dataDir: join(repo.root, 'repo-data'), defaultPath: repo.dir })
    const other = join(repo.root, 'other')
    mkdirSync(other)
    runGit(other, 'init', '-b', 'main')
    runGit(other, 'commit', '--allow-empty', '-m', 'first')
    const trees = join(repo.root, 'other-trees')
    await repos.add({ path: other, worktreesDir: trees })
    const withRepos = new WorkspaceService({
      dataDir,
      tasks,
      worktreesDir: join(repo.root, 'worktrees'),
      repos,
    })
    const there = tasks.createTask({ title: 'Fix it there', repo: 'other' })
    const here = tasks.createTask({ title: 'Fix it here' })

    expect((await withRepos.create(there.id)).workspace).toBe(join(trees, there.id))
    expect((await withRepos.create(here.id)).workspace).toBe(join(repo.root, 'worktrees', here.id))
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

  // The main checkout was not pulled: a new task starts from its remote branch, fetched now.
  it(
    'starts a new workspace from the newest remote branch when the checkout is behind',
    async () => {
      const origin = addBareOrigin(repo)
      runGit(repo.dir, 'push', '-u', 'origin', 'main')
      // Someone else pushes a commit, and the checkout does not pull it.
      const other = join(repo.root, 'other')
      runGit(repo.root, 'clone', origin, other)
      writeFileSync(join(other, 'newer.txt'), 'newer\n')
      runGit(other, 'add', 'newer.txt')
      runGit(other, 'commit', '-m', 'newer')
      runGit(other, 'push', 'origin', 'main')
      const newer = runGit(other, 'rev-parse', 'HEAD').trim()
      const before = runGit(repo.dir, 'rev-parse', 'HEAD').trim()

      const created = await workspaces.create(tasks.createTask({ title: 'Fresh' }).id)

      expect(runGit(created.workspace ?? '', 'rev-parse', 'HEAD').trim()).toBe(newer)
      expect(runGit(repo.dir, 'rev-parse', 'HEAD').trim()).toBe(before)
      // The task branch follows no branch, so a plain push never goes to main.
      expect(() => runGit(created.workspace ?? '', 'rev-parse', '@{upstream}')).toThrow()
      // The diff counts the task's own work, not the commits the checkout did not pull.
      expect((await workspaces.diffStats())[created.id]).toEqual({ files: 0, added: 0, removed: 0 })
    },
    GIT_TEST_MS,
  )

  it(
    'keeps local commits that are not pushed, and works without a remote or a network',
    async () => {
      // No remote: the checkout's HEAD, as before.
      const head = runGit(repo.dir, 'rev-parse', 'HEAD').trim()
      const first = await workspaces.create(tasks.createTask({ title: 'No remote' }).id)
      expect(runGit(first.workspace ?? '', 'rev-parse', 'HEAD').trim()).toBe(head)

      // A remote that is gone, and a local commit ahead of what was pushed.
      const origin = addBareOrigin(repo)
      runGit(repo.dir, 'push', '-u', 'origin', 'main')
      runGit(repo.dir, 'commit', '--allow-empty', '-m', 'local only')
      const local = runGit(repo.dir, 'rev-parse', 'HEAD').trim()
      rmSync(origin, { recursive: true, force: true })

      const second = await workspaces.create(tasks.createTask({ title: 'Offline' }).id)
      expect(runGit(second.workspace ?? '', 'rev-parse', 'HEAD').trim()).toBe(local)
    },
    GIT_TEST_MS,
  )

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

  it('deletes the branch of a merged pull request once origin has all of it', async () => {
    addBareOrigin(repo)
    const task = tasks.createTask({ title: 'Fix the bug' })
    const created = await workspaces.create(task.id)
    runGit(created.workspace ?? '', 'commit', '--allow-empty', '-m', 'work')
    runGit(created.workspace ?? '', 'push', 'origin', 'natamux/t-0001-fix-the-bug')
    tasks.recordPullRequestStatus(task.id, { state: 'merged', checks: 'passing' })

    const result = await workspaces.remove(task.id, { deleteBranch: true })

    expect(result.branchDeleted).toBe(true)
    expect(branchExists('natamux/t-0001-fix-the-bug')).toBe(false)
  })

  it('keeps the branch of a merged pull request that has a commit origin lacks', async () => {
    addBareOrigin(repo)
    const task = tasks.createTask({ title: 'Fix the bug' })
    const created = await workspaces.create(task.id)
    runGit(created.workspace ?? '', 'commit', '--allow-empty', '-m', 'work')
    runGit(created.workspace ?? '', 'push', 'origin', 'natamux/t-0001-fix-the-bug')
    runGit(created.workspace ?? '', 'commit', '--allow-empty', '-m', 'after the merge')
    tasks.recordPullRequestStatus(task.id, { state: 'merged', checks: 'passing' })

    const result = await workspaces.remove(task.id, { deleteBranch: true })

    expect(result.branchDeleted).toBe(false)
    expect(branchExists('natamux/t-0001-fix-the-bug')).toBe(true)
  })

  it('refuses a worktree with changes not committed, before ending its shells', async () => {
    const closed: string[] = []
    const guarded = new WorkspaceService({
      dataDir,
      tasks,
      repoDir: repo.dir,
      worktreesDir: join(repo.root, 'worktrees'),
      closeTerminals: (taskId) => closed.push(taskId),
    })
    const task = tasks.createTask({ title: 'Fix the bug' })
    const created = await guarded.create(task.id)
    writeFileSync(join(created.workspace ?? '', 'notes.txt'), 'work in progress')

    const refusal = guarded.remove(task.id, { deleteBranch: true })
    await expect(refusal).rejects.toThrow(WorkspaceConflictError)
    await expect(refusal).rejects.toThrow(
      `${task.id} has 1 change that is not committed. Commit or discard it first`,
    )

    expect(closed).toEqual([])
    expect(tasks.getTask(task.id).workspace).toBe(created.workspace)
    expect(existsSync(join(created.workspace ?? '', 'notes.txt'))).toBe(true)

    runGit(created.workspace ?? '', 'add', 'notes.txt')
    runGit(created.workspace ?? '', 'commit', '-m', 'Notes')
    await guarded.remove(task.id, { deleteBranch: false })
    expect(closed).toEqual([task.id])
  })

  describe('stage files', () => {
    const archived = (taskId: string) =>
      readFileSync(join(archiveDir(dataDir, taskId), 'spec.md'), 'utf8')

    async function withSpec(service: WorkspaceService, text: string) {
      const task = tasks.createTask({ title: 'Fix the bug' })
      const created = await service.create(task.id)
      const workspace = created.workspace ?? ''
      await excludeStageDir(workspace)
      mkdirSync(archiveDir(dataDir, task.id), { recursive: true })
      writeFileSync(join(archiveDir(dataDir, task.id), 'spec.md'), 'spec v1\n')
      mkdirSync(join(workspace, STAGE_DIR), { recursive: true })
      writeFileSync(join(workspace, STAGE_DIR, 'spec.md'), text)
      return { task, workspace }
    }

    it('archives the stage files that no stage archived yet', async () => {
      const { task } = await withSpec(workspaces, 'spec v2\n')

      await workspaces.remove(task.id, { deleteBranch: false })

      expect(archived(task.id)).toBe('spec v2\n')
    })

    it('archives a stage file the agent wrote while its shells ended', async () => {
      let workspace = ''
      const service = new WorkspaceService({
        dataDir,
        tasks,
        repoDir: repo.dir,
        worktreesDir: join(repo.root, 'worktrees'),
        closeTerminals: () => writeFileSync(join(workspace, STAGE_DIR, 'spec.md'), 'spec v3\n'),
      })
      const made = await withSpec(service, 'spec v2\n')
      workspace = made.workspace

      await service.remove(made.task.id, { deleteBranch: false })

      expect(archived(made.task.id)).toBe('spec v3\n')
    })

    it('leaves the archive alone when the removal is refused', async () => {
      const { task, workspace } = await withSpec(workspaces, 'spec v2\n')
      writeFileSync(join(workspace, 'notes.txt'), 'work in progress')

      await expect(workspaces.remove(task.id, { deleteBranch: false })).rejects.toThrow(
        WorkspaceConflictError,
      )

      expect(archived(task.id)).toBe('spec v1\n')
    })

    it('keeps the worktree and stops the agent when the archive cannot be written', async () => {
      const { task, workspace } = await withSpec(workspaces, 'spec v2\n')
      tasks.applyEvent(task.id, { source: 'human', type: 'task.readied' })
      tasks.applyEvent(task.id, { source: 'agent', type: 'agent.started' })
      // A file where the archive folder should be makes the copy fail.
      rmSync(archiveDir(dataDir, task.id), { recursive: true })
      writeFileSync(archiveDir(dataDir, task.id), 'not a folder')

      await expect(workspaces.remove(task.id, { deleteBranch: false })).rejects.toThrow()

      expect(readFileSync(join(workspace, STAGE_DIR, 'spec.md'), 'utf8')).toBe('spec v2\n')
      expect(tasks.getTask(task.id)).toMatchObject({ state: 'ready', workspace })
    })
  })

  it('makes the stage of the agent it ends wait to start again', async () => {
    const task = tasks.createTask({ title: 'Fix the bug' })
    await workspaces.create(task.id)
    tasks.applyEvent(task.id, { source: 'human', type: 'task.readied' })
    tasks.applyEvent(task.id, { source: 'agent', type: 'agent.started' })

    const result = await workspaces.remove(task.id, { deleteBranch: false })

    expect(result.task.state).toBe('ready')
    expect(tasks.getTask(task.id).state).toBe('ready')
    expect(eventTypes(task.id).slice(-2)).toEqual(['workspace.removed', 'agent.stopped'])
  })

  it('also stops an agent that ended its turn while the worktree was removed', async () => {
    const task = tasks.createTask({ title: 'Fix the bug' })
    await workspaces.create(task.id)
    tasks.applyEvent(task.id, { source: 'human', type: 'task.readied' })
    tasks.applyEvent(task.id, { source: 'agent', type: 'agent.started' })
    const ending = new WorkspaceService({
      dataDir,
      tasks,
      repoDir: repo.dir,
      worktreesDir: join(repo.root, 'worktrees'),
      // The agent's Stop hook as its shell ends.
      closeTerminals: (taskId) => {
        tasks.applyEvent(taskId, { source: 'agent', type: 'agent.finished' })
      },
    })

    const result = await ending.remove(task.id, { deleteBranch: false })

    expect(result.task.state).toBe('ready')
  })

  it('makes a stage whose agent waited for you wait to start again', async () => {
    const task = tasks.createTask({ title: 'Fix the bug' })
    await workspaces.create(task.id)
    tasks.applyEvent(task.id, { source: 'human', type: 'task.readied' })
    tasks.applyEvent(task.id, { source: 'agent', type: 'agent.started' })
    tasks.applyEvent(task.id, { source: 'agent', type: 'agent.asked' })

    const result = await workspaces.remove(task.id, { deleteBranch: false })

    expect(result.task.state).toBe('ready')
  })

  // Failed, blocked and done do not depend on a live agent: the task keeps them.
  it.each(['failed', 'blocked', 'done'] as const)('keeps a task that is %s', async (state) => {
    const task = tasks.createTask({ title: 'Fix the bug' })
    await workspaces.create(task.id)
    tasks.applyEvent(task.id, { source: 'human', type: 'state.overridden', payload: { to: state } })

    const result = await workspaces.remove(task.id, { deleteBranch: false })

    expect(result.task.state).toBe(state)
    expect(eventTypes(task.id).at(-1)).toBe('workspace.removed')
  })

  it('keeps the state of a running task when the removal is refused', async () => {
    const task = tasks.createTask({ title: 'Fix the bug' })
    const created = await workspaces.create(task.id)
    tasks.applyEvent(task.id, { source: 'human', type: 'task.readied' })
    tasks.applyEvent(task.id, { source: 'agent', type: 'agent.started' })
    writeFileSync(join(created.workspace ?? '', 'notes.txt'), 'work in progress')

    await expect(workspaces.remove(task.id, { deleteBranch: false })).rejects.toThrow(
      WorkspaceConflictError,
    )

    expect(tasks.getTask(task.id)).toMatchObject({ state: 'running', workspace: created.workspace })
  })

  it('refuses a worktree whose status git cannot read, before ending its shells', async () => {
    const { service, closed } = serviceWithShells()
    const task = tasks.createTask({ title: 'Fix the bug' })
    const created = await service.create(task.id)
    // Windows marks .git hidden, and a hidden file cannot be opened for writing.
    rmSync(join(created.workspace ?? '', '.git'))
    writeFileSync(join(created.workspace ?? '', '.git'), 'not a git file\n')

    await expect(service.remove(task.id, { deleteBranch: false })).rejects.toThrow(GitError)

    expect(closed).toEqual([])
    expect(tasks.getTask(task.id).workspace).toBe(created.workspace)
  })

  it('stops the agent when git refuses the worktree after its shells ended', async () => {
    const { service, closed } = serviceWithShells()
    const task = tasks.createTask({ title: 'Fix the bug' })
    const created = await service.create(task.id)
    tasks.applyEvent(task.id, { source: 'human', type: 'task.readied' })
    tasks.applyEvent(task.id, { source: 'agent', type: 'agent.started' })
    runGit(repo.dir, 'worktree', 'lock', created.workspace ?? '')

    await expect(service.remove(task.id, { deleteBranch: false })).rejects.toThrow(/locked/)

    expect(closed).toEqual([task.id])
    expect(tasks.getTask(task.id)).toMatchObject({ state: 'ready', workspace: created.workspace })
  })

  it(
    'removes a clean worktree with a submodule checked out',
    async () => {
      const lib = join(repo.root, 'lib')
      mkdirSync(lib)
      runGit(lib, 'init', '-b', 'main')
      runGit(lib, 'commit', '--allow-empty', '-m', 'lib')
      const task = tasks.createTask({ title: 'Fix the bug' })
      const path = (await workspaces.create(task.id)).workspace ?? ''
      runGit(path, '-c', 'protocol.file.allow=always', 'submodule', 'add', lib, 'lib')
      runGit(path, 'commit', '-m', 'Add lib')

      const result = await workspaces.remove(task.id, { deleteBranch: false })

      expect(result.task.workspace).toBeNull()
      expect(existsSync(path)).toBe(false)
    },
    GIT_TEST_MS,
  )

  it(
    'removes a workspace made again the same way',
    async () => {
      const task = tasks.createTask({ title: 'Fix the bug' })
      await workspaces.create(task.id)
      tasks.applyEvent(task.id, { source: 'human', type: 'task.readied' })
      tasks.applyEvent(task.id, { source: 'agent', type: 'agent.started' })
      await workspaces.remove(task.id, { deleteBranch: false })

      const again = await workspaces.create(task.id)
      expect(again).toMatchObject({
        state: 'ready',
        workspace: workspaces.worktreePathFor(task),
      })
      tasks.applyEvent(task.id, { source: 'agent', type: 'agent.started' })

      const result = await workspaces.remove(task.id, { deleteBranch: false })

      expect(result.task).toMatchObject({ state: 'ready', workspace: null })
      expect(eventTypes(task.id).filter((type) => type === 'agent.stopped')).toHaveLength(2)
    },
    GIT_TEST_MS,
  )

  it('keeps a stage that waits for approval', async () => {
    const task = tasks.createTask({ title: 'Fix the bug' })
    await workspaces.create(task.id)
    tasks.applyEvent(task.id, {
      source: 'human',
      type: 'state.overridden',
      payload: { to: 'review' },
    })

    const result = await workspaces.remove(task.id, { deleteBranch: false })

    expect(result.task.state).toBe('review')
    expect(eventTypes(task.id).at(-1)).toBe('workspace.removed')
  })

  it('refuses a task that has no workspace', async () => {
    const task = tasks.createTask({ title: 'Fix the bug' })

    await expect(workspaces.remove(task.id, { deleteBranch: true })).rejects.toThrow(
      WorkspaceConflictError,
    )
  })

  // A folder deleted outside Natamux: `pruned` when git forgot it too, else git still lists it.
  function deleteFolder(path: string, pruned: boolean): void {
    rmSync(path, { recursive: true, force: true })
    if (pruned) {
      runGit(repo.dir, 'worktree', 'prune')
    }
  }

  function eventTypes(taskId: string): string[] {
    return tasks.listEvents(taskId).map((stored) => stored.event.type)
  }

  // What is at a path: the names in a folder, or the text of a file.
  function contentsOf(path: string): string[] {
    return statSync(path).isDirectory() ? readdirSync(path).sort() : [readFileSync(path, 'utf8')]
  }

  // A service that writes down whose shells were ended, and the workspace the task had then.
  function serviceWithShells(worktreesDir = join(repo.root, 'worktrees')) {
    const closed: string[] = []
    const workspaceWhenClosed: (string | null)[] = []
    const service = new WorkspaceService({
      dataDir,
      tasks,
      repoDir: repo.dir,
      worktreesDir,
      closeTerminals: (taskId) => {
        closed.push(taskId)
        workspaceWhenClosed.push(tasks.getTask(taskId).workspace)
      },
    })
    return { service, closed, workspaceWhenClosed }
  }

  const deletedCases: [string, boolean][] = [
    ['pruned', true],
    ['still listed by git', false],
  ]

  it('clears a workspace whose folder was deleted and pruned', async () => {
    const task = tasks.createTask({ title: 'Fix the bug' })
    const created = await workspaces.create(task.id)
    deleteFolder(created.workspace ?? '', true)

    const result = await workspaces.remove(task.id, { deleteBranch: false })

    expect(result.branchDeleted).toBe(false)
    expect(result.task.workspace).toBeNull()
    expect(result.task.branch).toBe('natamux/t-0001-fix-the-bug')
    expect(branchExists('natamux/t-0001-fix-the-bug')).toBe(true)
    expect(tasks.listEvents(task.id).at(-1)?.event.type).toBe('workspace.removed')
  })

  it('clears a deleted workspace when its parent folder is gone too', async () => {
    const task = tasks.createTask({ title: 'Fix the bug' })
    await workspaces.create(task.id)
    rmSync(join(repo.root, 'worktrees'), { recursive: true, force: true })
    runGit(repo.dir, 'worktree', 'prune')

    const result = await workspaces.remove(task.id, { deleteBranch: false })

    expect(result.task.workspace).toBeNull()
    expect(eventTypes(task.id).at(-1)).toBe('workspace.removed')
  })

  it('refuses a second remove after a deleted folder was cleared', async () => {
    const task = tasks.createTask({ title: 'Fix the bug' })
    const created = await workspaces.create(task.id)
    deleteFolder(created.workspace ?? '', true)
    await workspaces.remove(task.id, { deleteBranch: false })

    await expect(workspaces.remove(task.id, { deleteBranch: false })).rejects.toThrow(
      WorkspaceConflictError,
    )
    expect(eventTypes(task.id).filter((type) => type === 'workspace.removed')).toHaveLength(1)
  })

  it.each([false, true])(
    'clears a deleted folder whose branch was deleted by hand too (deleteBranch: %s)',
    async (deleteBranch) => {
      const task = tasks.createTask({ title: 'Fix the bug' })
      const created = await workspaces.create(task.id)
      deleteFolder(created.workspace ?? '', true)
      runGit(repo.dir, 'branch', '-D', 'natamux/t-0001-fix-the-bug')

      const result = await workspaces.remove(task.id, { deleteBranch })

      expect(result.branchDeleted).toBe(false)
      expect(result.task.workspace).toBeNull()
      expect(eventTypes(task.id).at(-1)).toBe('workspace.removed')
    },
  )

  it('clears the git entry of a workspace whose folder was deleted', async () => {
    const task = tasks.createTask({ title: 'Fix the bug' })
    const created = await workspaces.create(task.id)
    const path = created.workspace ?? ''
    deleteFolder(path, false)
    expect(await worktreeState(repo.dir, path)).toBe('stale')

    const result = await workspaces.remove(task.id, { deleteBranch: false })

    expect(result.task.workspace).toBeNull()
    expect(result.task.branch).toBe('natamux/t-0001-fix-the-bug')
    expect(await worktreeState(repo.dir, path)).toBe('missing')
    expect(tasks.listEvents(task.id).at(-1)?.event.type).toBe('workspace.removed')
  })

  it('clears the git entry of a deleted folder whose path has a space', async () => {
    const { service } = serviceWithShells(join(repo.root, 'Application Support', 'worktrees'))
    const task = tasks.createTask({ title: 'Fix the bug' })
    const created = await service.create(task.id)
    const path = created.workspace ?? ''
    deleteFolder(path, false)
    expect(await worktreeState(repo.dir, path)).toBe('stale')

    const result = await service.remove(task.id, { deleteBranch: false })

    expect(result.task.workspace).toBeNull()
    expect(await worktreeState(repo.dir, path)).toBe('missing')
  })

  it.each(deletedCases)(
    'leaves the worktree of another task alone after a deleted folder (%s)',
    async (_, pruned) => {
      const task = tasks.createTask({ title: 'Fix the bug' })
      const other = tasks.createTask({ title: 'Other work' })
      const created = await workspaces.create(task.id)
      const otherPath = (await workspaces.create(other.id)).workspace ?? ''
      writeFileSync(join(otherPath, 'notes.txt'), 'work in progress')
      deleteFolder(created.workspace ?? '', pruned)

      await workspaces.remove(task.id, { deleteBranch: false })

      expect(await worktreeState(repo.dir, otherPath)).toBe('present')
      expect(existsSync(join(otherPath, 'notes.txt'))).toBe(true)
      expect(tasks.getTask(other.id).workspace).toBe(otherPath)
    },
  )

  it('refuses a locked entry of a deleted folder, and leaves the task as it was', async () => {
    const task = tasks.createTask({ title: 'Fix the bug' })
    const created = await workspaces.create(task.id)
    const path = created.workspace ?? ''
    runGit(repo.dir, 'worktree', 'lock', path)
    deleteFolder(path, false)

    await expect(workspaces.remove(task.id, { deleteBranch: true })).rejects.toThrow(/locked/)

    expect(tasks.getTask(task.id).workspace).toBe(path)
    expect(eventTypes(task.id)).not.toContain('workspace.removed')
    expect(await worktreeState(repo.dir, path)).toBe('stale')
    expect(branchExists('natamux/t-0001-fix-the-bug')).toBe(true)
  })

  it.each(deletedCases)('deletes a merged branch of a deleted folder (%s)', async (_, pruned) => {
    const task = tasks.createTask({ title: 'Fix the bug' })
    const created = await workspaces.create(task.id)
    deleteFolder(created.workspace ?? '', pruned)

    const result = await workspaces.remove(task.id, { deleteBranch: true })

    expect(result.branchDeleted).toBe(true)
    expect(result.task.workspace).toBeNull()
    expect(result.task.branch).toBeNull()
    expect(branchExists('natamux/t-0001-fix-the-bug')).toBe(false)
    expect(eventTypes(task.id).at(-1)).toBe('workspace.removed')
  })

  it.each(deletedCases)(
    'keeps the branch of a deleted folder that has commits not merged (%s)',
    async (_, pruned) => {
      const task = tasks.createTask({ title: 'Fix the bug' })
      const created = await workspaces.create(task.id)
      runGit(created.workspace ?? '', 'commit', '--allow-empty', '-m', 'work')
      deleteFolder(created.workspace ?? '', pruned)

      const result = await workspaces.remove(task.id, { deleteBranch: true })

      expect(result.branchDeleted).toBe(false)
      expect(result.task.workspace).toBeNull()
      expect(result.task.branch).toBe('natamux/t-0001-fix-the-bug')
      expect(branchExists('natamux/t-0001-fix-the-bug')).toBe(true)
      const subject = runGit(repo.dir, 'log', '-1', '--format=%s', 'natamux/t-0001-fix-the-bug')
      expect(subject.trim()).toBe('work')
      expect(eventTypes(task.id).at(-1)).toBe('workspace.removed')
    },
  )

  it.each(deletedCases)(
    'ends the shells of a deleted folder once, before the task is cleared (%s)',
    async (_, pruned) => {
      const { service, closed, workspaceWhenClosed } = serviceWithShells()
      const task = tasks.createTask({ title: 'Fix the bug' })
      const other = tasks.createTask({ title: 'Other work' })
      const created = await service.create(task.id)
      await service.create(other.id)
      deleteFolder(created.workspace ?? '', pruned)

      await service.remove(task.id, { deleteBranch: false })

      expect(closed).toEqual([task.id])
      expect(workspaceWhenClosed).toEqual([created.workspace])
    },
  )

  it.each(deletedCases)(
    'makes a new workspace at the same path after a deleted folder (%s)',
    async (_, pruned) => {
      const task = tasks.createTask({ title: 'Fix the bug' })
      const created = await workspaces.create(task.id)
      runGit(created.workspace ?? '', 'commit', '--allow-empty', '-m', 'work')
      deleteFolder(created.workspace ?? '', pruned)
      await workspaces.remove(task.id, { deleteBranch: true })

      const again = await workspaces.create(task.id)

      expect(again.workspace).toBe(created.workspace)
      expect(again.branch).toBe('natamux/t-0001-fix-the-bug')
      expect(runGit(again.workspace ?? '', 'log', '-1', '--format=%s').trim()).toBe('work')
      expect(await worktreeState(repo.dir, again.workspace ?? '')).toBe('present')
    },
  )

  it.each(deletedCases)(
    'goes on with the kept branch when a deleted folder is cleared without its branch (%s)',
    async (_, pruned) => {
      const task = tasks.createTask({ title: 'Fix the bug' })
      const created = await workspaces.create(task.id)
      deleteFolder(created.workspace ?? '', pruned)
      await workspaces.remove(task.id, { deleteBranch: false })

      const again = await workspaces.create(task.id)

      expect(again.workspace).toBe(created.workspace)
      expect(again.branch).toBe('natamux/t-0001-fix-the-bug')
      expect(existsSync(again.workspace ?? '')).toBe(true)
      expect(await worktreeState(repo.dir, again.workspace ?? '')).toBe('present')
    },
  )

  it.each(deletedCases)(
    'makes a new branch after a deleted folder whose merged branch was deleted (%s)',
    async (_, pruned) => {
      const task = tasks.createTask({ title: 'Fix the bug' })
      const created = await workspaces.create(task.id)
      deleteFolder(created.workspace ?? '', pruned)
      await workspaces.remove(task.id, { deleteBranch: true })

      const again = await workspaces.create(task.id)

      expect(again.workspace).toBe(created.workspace)
      expect(again.branch).toBe('natamux/t-0001-fix-the-bug')
      expect(runGit(again.workspace ?? '', 'log', '-1', '--format=%s').trim()).toBe('init')
      expect(await worktreeState(repo.dir, again.workspace ?? '')).toBe('present')
    },
  )

  // Each one puts something at the path that is not a worktree of the repository.
  const foreignCases: [string, (path: string) => void][] = [
    [
      'a folder with a file',
      (path) => {
        mkdirSync(path)
        writeFileSync(join(path, 'notes.txt'), 'my own files')
      },
    ],
    ['an empty folder', (path) => mkdirSync(path)],
    [
      'a worktree of another repository',
      (path) => {
        const other = join(repo.root, 'other')
        runGit(repo.root, 'init', '-b', 'main', other)
        runGit(other, 'commit', '--allow-empty', '-m', 'other')
        runGit(other, 'worktree', 'add', path)
      },
    ],
    ['a git repository of its own', (path) => runGit(repo.root, 'init', path)],
    ['a file', (path) => writeFileSync(path, 'not a folder')],
  ]

  it.each(foreignCases)(
    'refuses a path that is %s instead of a worktree, and leaves it alone',
    async (_, make) => {
      const { service, closed } = serviceWithShells()
      const task = tasks.createTask({ title: 'Fix the bug' })
      const created = await service.create(task.id)
      const path = created.workspace ?? ''
      deleteFolder(path, true)
      make(path)
      const before = contentsOf(path)

      const error = await service.remove(task.id, { deleteBranch: true }).catch((e: unknown) => e)

      expect(error).toBeInstanceOf(WorkspaceConflictError)
      expect((error as Error).message).toContain(path)
      expect((error as Error).message).toContain('not a git worktree')
      expect(closed).toEqual([])
      expect(contentsOf(path)).toEqual(before)
      expect(tasks.getTask(task.id).workspace).toBe(path)
      expect(eventTypes(task.id)).not.toContain('workspace.removed')
      expect(branchExists('natamux/t-0001-fix-the-bug')).toBe(true)
    },
  )

  // Commits a file in the worktree, so that a change to it is a change to a tracked file.
  function commitTrackedFile(path: string): void {
    writeFileSync(join(path, 'tracked.txt'), 'first')
    runGit(path, 'add', 'tracked.txt')
    runGit(path, 'commit', '-m', 'tracked')
  }

  function editAndAddFile(path: string): void {
    writeFileSync(join(path, 'tracked.txt'), 'edited')
    writeFileSync(join(path, 'new.txt'), 'new')
  }

  function stageFile(path: string): void {
    writeFileSync(join(path, 'staged.txt'), 'staged')
    runGit(path, 'add', 'staged.txt')
  }

  const dirtyCases: [string, boolean, (path: string) => void][] = [
    ['a tracked edit and a new file', false, editAndAddFile],
    ['a tracked edit and a new file', true, editAndAddFile],
    ['a staged change only', false, stageFile],
  ]

  it.each(dirtyCases)(
    'refuses a worktree with %s (deleteBranch: %s) and leaves it as it was',
    async (_, deleteBranch, change) => {
      const { service, closed } = serviceWithShells()
      const task = tasks.createTask({ title: 'Fix the bug' })
      const created = await service.create(task.id)
      const path = created.workspace ?? ''
      commitTrackedFile(path)
      change(path)
      const status = runGit(path, 'status', '--porcelain')

      await expect(service.remove(task.id, { deleteBranch })).rejects.toThrow(
        WorkspaceConflictError,
      )

      expect(runGit(path, 'status', '--porcelain')).toBe(status)
      expect(closed).toEqual([])
      expect(tasks.getTask(task.id)).toMatchObject({
        workspace: path,
        branch: 'natamux/t-0001-fix-the-bug',
      })
      expect(eventTypes(task.id)).not.toContain('workspace.removed')
      expect(branchExists('natamux/t-0001-fix-the-bug')).toBe(true)
    },
  )

  it('ends the shells once and drops the git entry of a worktree that is there', async () => {
    const { service, closed } = serviceWithShells()
    const task = tasks.createTask({ title: 'Fix the bug' })
    const created = await service.create(task.id)
    const path = created.workspace ?? ''

    const result = await service.remove(task.id, { deleteBranch: false })

    expect(closed).toEqual([task.id])
    expect(existsSync(path)).toBe(false)
    expect(await worktreeState(repo.dir, path)).toBe('missing')
    expect(result.task.workspace).toBeNull()
    expect(eventTypes(task.id).at(-1)).toBe('workspace.removed')
  })

  it('removes a worktree that has ignored files only', async () => {
    const task = tasks.createTask({ title: 'Fix the bug' })
    const created = await workspaces.create(task.id)
    const path = created.workspace ?? ''
    writeFileSync(join(path, '.gitignore'), '*.log\n')
    runGit(path, 'add', '.gitignore')
    runGit(path, 'commit', '-m', 'ignore logs')
    writeFileSync(join(path, 'debug.log'), 'noise')

    const result = await workspaces.remove(task.id, { deleteBranch: false })

    expect(result.task.workspace).toBeNull()
    expect(existsSync(path)).toBe(false)
  })

  it('passes on a git failure for a locked worktree, and leaves the task as it was', async () => {
    const task = tasks.createTask({ title: 'Fix the bug' })
    const created = await workspaces.create(task.id)
    const path = created.workspace ?? ''
    runGit(repo.dir, 'worktree', 'lock', path)

    await expect(workspaces.remove(task.id, { deleteBranch: false })).rejects.toThrow(
      /git worktree failed: .*locked/,
    )

    expect(existsSync(path)).toBe(true)
    expect(tasks.getTask(task.id).workspace).toBe(path)
    expect(eventTypes(task.id)).not.toContain('workspace.removed')
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

  it('keeps the graph for a moment, and reads it again after a commit made here', async () => {
    const task = tasks.createTask({ title: 'Fix the bug' })
    const created = await workspaces.create(task.id)
    const first = await workspaces.graph(task.id)
    runGit(created.workspace ?? '', 'commit', '--allow-empty', '-m', 'outside')

    expect(await workspaces.graph(task.id)).toBe(first)

    writeFileSync(join(created.workspace ?? '', 'fix.txt'), 'fixed\n')
    await workspaces.commit(task.id, 'Fix the bug')
    expect((await workspaces.graph(task.id)).ahead).toBe(2)
  })

  it('reads git now when asked for a fresh graph, and keeps that one', async () => {
    const task = tasks.createTask({ title: 'Fix the bug' })
    const created = await workspaces.create(task.id)
    await workspaces.graph(task.id)
    runGit(created.workspace ?? '', 'commit', '--allow-empty', '-m', 'outside')

    const fresh = await workspaces.graph(task.id, true)
    expect(fresh.ahead).toBe(1)
    expect(await workspaces.graph(task.id)).toBe(fresh)
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
    clock += GIT_READ_MS

    expect(await workspaces.changes(task.id)).toEqual([
      { path: 'notes.txt', status: 'U', originalPath: null, added: 1, removed: 0 },
    ])
  })

  it('keeps the list for a moment, then reads it again', async () => {
    const { task, workspace } = await taskWithWorkspace()
    const first = await workspaces.changes(task.id)
    writeFileSync(join(workspace, 'notes.txt'), 'hello\n')

    clock += GIT_READ_MS - 1
    expect(await workspaces.changes(task.id)).toBe(first)
    clock += 1
    expect((await workspaces.changes(task.id)).map((entry) => entry.path)).toEqual(['notes.txt'])
  })

  it('reads git now when asked for a fresh list, and keeps that one', async () => {
    const { task, workspace } = await taskWithWorkspace()
    await workspaces.changes(task.id)
    writeFileSync(join(workspace, 'notes.txt'), 'hello\n')

    const fresh = await workspaces.changes(task.id, true)
    expect(fresh.map((entry) => entry.path)).toEqual(['notes.txt'])
    expect(await workspaces.changes(task.id)).toBe(fresh)
  })

  it('shares one read between callers that ask at the same time', async () => {
    const { task } = await taskWithWorkspace()
    const first = workspaces.changes(task.id)
    const second = workspaces.changes(task.id)
    expect(await second).toBe(await first)
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
    // As a hook or shell of the task does after a change.
    workspaces.touch(task.id)

    await expect(workspaces.fileDiff(task.id, 'notes.txt')).rejects.toThrow(FileNotChangedError)
  })

  it('reuses the kept list of changes for the diff of a file', async () => {
    const { task, workspace } = await taskWithWorkspace()
    writeFileSync(join(workspace, 'tracked.txt'), 'one\n')
    runGit(workspace, 'add', 'tracked.txt')
    runGit(workspace, 'commit', '-m', 'add tracked')
    writeFileSync(join(workspace, 'tracked.txt'), 'two\n')
    await workspaces.changes(task.id)

    const before = gitSpawnCount()
    expect((await workspaces.fileDiff(task.id, 'tracked.txt')).modified).toBe('two\n')
    // `cat-file -s` and `show` only, no git status.
    expect(gitSpawnCount() - before).toBe(2)
  })

  it('reads the changes again for a file that changed after the kept list', async () => {
    const { task, workspace } = await taskWithWorkspace()
    await workspaces.changes(task.id)
    writeFileSync(join(workspace, 'notes.txt'), 'hello\n')

    expect((await workspaces.fileDiff(task.id, 'notes.txt')).modified).toBe('hello\n')
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
      dataDir,
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

    // Kept from before the commit, which must not show up afterwards.
    expect(await workspaces.changes(task.id)).toHaveLength(1)
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

  it('pushes the parent branch and opens the pull request of a subtask into it', async () => {
    const remote = addBareOrigin(repo)
    const parent = await workspaces.create(tasks.createTask({ title: 'Parent' }).id)
    const child = tasks.createTask({ title: 'Part A', parentId: parent.id })
    const created = await workspaces.create(child.id)
    writeFileSync(join(created.workspace ?? '', 'part.txt'), 'part\n')
    await workspaces.commit(child.id, 'Part A')

    await workspaces.openPullRequest(child.id)

    expect(runGit(remote, 'branch', '--list', parent.branch ?? '').trim()).not.toBe('')
    expect(ghCalls).toEqual([
      [
        'pr',
        'create',
        '--head',
        created.branch,
        '--base',
        parent.branch,
        '--title',
        'Part A',
        '--body',
        `Task ${child.id} in Natamux.`,
      ],
    ])
  })

  it('opens a later subtask pull request when origin has the parent branch ahead', async () => {
    const remote = addBareOrigin(repo)
    const parent = await workspaces.create(tasks.createTask({ title: 'Parent' }).id)
    runGit(parent.workspace ?? '', 'push', '-u', 'origin', parent.branch ?? '')
    // A subtask merged into the parent branch on GitHub: the local copy is behind.
    const other = join(repo.root, 'other')
    runGit(repo.root, 'clone', '--branch', parent.branch ?? '', remote, other)
    runGit(other, 'commit', '--allow-empty', '-m', 'merged subtask')
    runGit(other, 'push', 'origin', parent.branch ?? '')
    const merged = runGit(other, 'rev-parse', 'HEAD').trim()
    const child = tasks.createTask({ title: 'Part B', parentId: parent.id })
    const created = await workspaces.create(child.id)
    writeFileSync(join(created.workspace ?? '', 'part.txt'), 'part\n')
    await workspaces.commit(child.id, 'Part B')

    const updated = await workspaces.openPullRequest(child.id)

    expect(updated.pr).toBe(PR_URL)
    expect(ghCalls[0]?.slice(4, 6)).toEqual(['--base', parent.branch])
    expect(runGit(remote, 'rev-parse', parent.branch ?? '').trim()).toBe(merged)
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

describe('draftCommitMessage', () => {
  let runs: AgentRun[]
  let answer: (run: AgentRun) => Promise<string>
  let logged: string[]

  beforeEach(() => {
    runs = []
    logged = []
    answer = async () => 'Add the fix\n'
    workspaces = new WorkspaceService({
      dataDir,
      log: (message) => logged.push(message),
      tasks,
      repoDir: repo.dir,
      worktreesDir: join(repo.root, 'worktrees'),
      programOf: (agent) => `my-${agent}`,
      runAgentOnce: (run) => {
        runs.push(run)
        return answer(run)
      },
    })
  })

  async function taskWithWork() {
    const task = tasks.createTask({ title: 'Fix the bug' })
    const created = await workspaces.create(task.id)
    writeFileSync(join(created.workspace ?? '', 'fix.txt'), 'fixed\n')
    return created
  }

  it('asks the task agent, with the diff, and leaves the worktree as it was', async () => {
    const task = await taskWithWork()
    tasks.recordAgentLaunch(task.id, 'codex')
    const workspace = task.workspace ?? ''
    const status = runGit(workspace, 'status', '--porcelain')
    const events = tasks.listEvents(task.id).length

    const draft = await workspaces.draftCommitMessage(task.id)

    expect(draft).toEqual({ message: 'Add the fix', source: 'agent', agent: 'codex' })
    expect(runs).toHaveLength(1)
    expect(runs[0]).toMatchObject({ agent: 'codex', program: 'my-codex', cwd: workspace })
    expect(runs[0]?.prompt).toContain('Task: Fix the bug')
    expect(runs[0]?.prompt).toContain('+fixed')
    expect(runGit(workspace, 'status', '--porcelain')).toBe(status)
    expect(tasks.listEvents(task.id)).toHaveLength(events)
    expect(tasks.getTask(task.id).state).toBe(task.state)
  })

  it.each([null, 'shell'])('uses Claude Code for a task whose agent is %j', async (agent) => {
    const task = await taskWithWork()
    if (agent !== null) {
      tasks.recordAgentLaunch(task.id, agent)
    }
    expect(await workspaces.draftCommitMessage(task.id)).toMatchObject({ agent: 'claude-code' })
    expect(runs[0]?.program).toBe('my-claude-code')
  })

  it('gives the simple draft when the agent fails', async () => {
    const task = await taskWithWork()
    answer = async () => {
      throw new Error('my-claude-code exited with code 127')
    }
    expect(await workspaces.draftCommitMessage(task.id)).toEqual({
      message: 'Add fix.txt',
      source: 'draft',
      agent: 'claude-code',
      reason: 'my-claude-code exited with code 127',
    })
    expect(logged).toEqual([
      `No commit message draft by claude-code for ${task.id}: my-claude-code exited with code 127`,
    ])
  })

  it('logs nothing when the agent drafts', async () => {
    const task = await taskWithWork()
    await workspaces.draftCommitMessage(task.id)
    expect(logged).toEqual([])
  })

  it('runs the agent once for two asks at the same time, and again after', async () => {
    const task = await taskWithWork()
    const [first, second] = await Promise.all([
      workspaces.draftCommitMessage(task.id),
      workspaces.draftCommitMessage(task.id),
    ])
    expect(second).toEqual(first)
    expect(runs).toHaveLength(1)

    await workspaces.draftCommitMessage(task.id)
    expect(runs).toHaveLength(2)
  })

  it('starts a new run after a failed one, and runs once per task', async () => {
    const one = await taskWithWork()
    answer = async () => {
      throw new Error('boom')
    }
    expect(await workspaces.draftCommitMessage(one.id)).toMatchObject({ source: 'draft' })
    answer = async () => 'Add the fix'
    expect(await workspaces.draftCommitMessage(one.id)).toMatchObject({ source: 'agent' })
    expect(runs).toHaveLength(2)

    const other = tasks.createTask({ title: 'Other' })
    const created = await workspaces.create(other.id)
    writeFileSync(join(created.workspace ?? '', 'other.txt'), 'x\n')
    await Promise.all([
      workspaces.draftCommitMessage(one.id),
      workspaces.draftCommitMessage(other.id),
    ])
    const cwds = new Set(runs.slice(2).map((run) => run.cwd))
    expect(cwds).toEqual(new Set([one.workspace, created.workspace]))
  })

  // An agent run that ends when the test says so.
  function heldAgent() {
    const held = { started: false, finish: (_text: string) => {} }
    const run = () =>
      new Promise<string>((done) => {
        held.started = true
        held.finish = done
      })
    return { held, run }
  }

  it('does not wait for a push, and does not hold a commit back', async () => {
    let release: () => void = () => {}
    const blocked = new Promise<void>((done) => {
      release = done
    })
    const gh: RunGh = async () => {
      await blocked
      return 'https://github.com/o/r/pull/1\n'
    }
    const first = heldAgent()
    workspaces = new WorkspaceService({
      dataDir,
      tasks,
      repoDir: repo.dir,
      worktreesDir: join(repo.root, 'worktrees'),
      gh,
      runAgentOnce: first.run,
    })
    addBareOrigin(repo)
    const task = await taskWithWork()
    await workspaces.commit(task.id, 'First')
    writeFileSync(join(task.workspace ?? '', 'more.txt'), 'more\n')

    const opening = workspaces.openPullRequest(task.id)
    const drafting = workspaces.draftCommitMessage(task.id)
    await vi.waitFor(() => expect(first.held.started).toBe(true), { timeout: 10_000 })
    first.held.finish('Add more')
    expect(await drafting).toMatchObject({ message: 'Add more' })
    release()
    await opening

    const second = heldAgent()
    workspaces = new WorkspaceService({
      dataDir,
      tasks,
      repoDir: repo.dir,
      worktreesDir: join(repo.root, 'worktrees'),
      runAgentOnce: second.run,
    })
    const waiting = workspaces.draftCommitMessage(task.id)
    await vi.waitFor(() => expect(second.held.started).toBe(true), { timeout: 10_000 })
    await workspaces.commit(task.id, 'Add more')
    second.held.finish('Late')
    expect(await waiting).toMatchObject({ message: 'Late' })
  })

  it('refuses when nothing changed, or there is no workspace', async () => {
    const task = tasks.createTask({ title: 'Fix the bug' })
    await expect(workspaces.draftCommitMessage(task.id)).rejects.toThrow(WorkspaceConflictError)
    await workspaces.create(task.id)
    await expect(workspaces.draftCommitMessage(task.id)).rejects.toThrow(NothingToCommitError)
    expect(runs).toEqual([])
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

  it('shares one read between callers, and keeps it for a moment', async () => {
    const task = tasks.createTask({ title: 'Fix the bug' })
    const created = await workspaces.create(task.id)

    const first = workspaces.diffStats()
    expect(workspaces.diffStats()).toBe(first)
    await first

    writeFileSync(join(created.workspace ?? '', 'notes.txt'), 'a\n')
    runGit(created.workspace ?? '', 'add', 'notes.txt')
    expect(workspaces.diffStats()).toBe(first)

    clock += GIT_READ_MS
    const next = workspaces.diffStats()
    expect(next).not.toBe(first)
    // Nothing told the service that the workspace changed.
    expect(await next).toEqual({ [task.id]: { files: 0, added: 0, removed: 0 } })

    clock += DIFF_STATS_MAX_AGE_MS
    expect(await workspaces.diffStats()).toEqual({ [task.id]: { files: 1, added: 1, removed: 0 } })
  })

  it('reads again only the workspaces that changed', async () => {
    const first = await workspaces.create(tasks.createTask({ title: 'First' }).id)
    const second = await workspaces.create(tasks.createTask({ title: 'Second' }).id)
    const none = { files: 0, added: 0, removed: 0 }
    expect(await workspaces.diffStats()).toEqual({ [first.id]: none, [second.id]: none })
    for (const task of [first, second]) {
      writeFileSync(join(task.workspace ?? '', 'notes.txt'), 'a\n')
      runGit(task.workspace ?? '', 'add', 'notes.txt')
    }

    workspaces.touch(first.id)

    expect(await workspaces.diffStats()).toEqual({
      [first.id]: { files: 1, added: 1, removed: 0 },
      [second.id]: none,
    })
  })

  it('runs git only for the workspaces that changed', { timeout: GIT_TEST_MS }, async () => {
    const ids: string[] = []
    for (const title of ['First', 'Second', 'Third']) {
      ids.push((await workspaces.create(tasks.createTask({ title }).id)).id)
    }
    // The git processes of one read after the given workspaces changed.
    const spawnsAfter = async (touched: readonly string[]) => {
      for (const id of touched) {
        workspaces.touch(id)
      }
      clock += GIT_READ_MS
      const before = gitSpawnCount()
      expect(Object.keys(await workspaces.diffStats())).toHaveLength(3)
      return gitSpawnCount() - before
    }

    const all = await spawnsAfter([])
    const two = await spawnsAfter(ids.slice(0, 2))
    const one = await spawnsAfter(ids.slice(0, 1))
    expect(await spawnsAfter([])).toBe(0)

    // Each workspace read again costs the same number of git processes.
    expect(two - one).toBeGreaterThan(0)
    expect(all - two).toBe(two - one)
  })

  it('keeps the stats of the other workspaces when git fails for one', async () => {
    const good = await workspaces.create(tasks.createTask({ title: 'Good' }).id)
    const broken = await workspaces.create(tasks.createTask({ title: 'Broken' }).id)
    const none = { files: 0, added: 0, removed: 0 }
    expect(await workspaces.diffStats()).toEqual({ [good.id]: none, [broken.id]: none })

    rmSync(broken.workspace ?? '', { recursive: true, force: true })
    workspaces.touch(broken.id)
    expect(await workspaces.diffStats()).toEqual({ [good.id]: none })

    // Nothing told the service, so the good one still comes from the earlier read.
    writeFileSync(join(good.workspace ?? '', 'notes.txt'), 'a\n')
    runGit(good.workspace ?? '', 'add', 'notes.txt')
    clock += GIT_READ_MS
    expect(await workspaces.diffStats()).toEqual({ [good.id]: none })

    workspaces.touch(good.id)
    expect(await workspaces.diffStats()).toEqual({ [good.id]: { files: 1, added: 1, removed: 0 } })
  })

  it('reads again at once after a workspace is made or removed here', async () => {
    const task = tasks.createTask({ title: 'Fix the bug' })
    expect(await workspaces.diffStats()).toEqual({})

    await workspaces.create(task.id)
    expect(await workspaces.diffStats()).toEqual({
      [task.id]: { files: 0, added: 0, removed: 0 },
    })

    await workspaces.remove(task.id, { deleteBranch: true })
    expect(await workspaces.diffStats()).toEqual({})
  })
})

describe('touch', () => {
  let sent: DaemonMessage[]
  let touching: WorkspaceService

  beforeEach(() => {
    sent = []
    const bus = new EventBus()
    bus.subscribe((message) => sent.push(message))
    touching = new WorkspaceService({
      dataDir,
      tasks,
      repoDir: repo.dir,
      worktreesDir: join(repo.root, 'worktrees'),
      bus,
      now: () => clock,
    })
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('tells the dashboards once per burst of changes in a task', () => {
    vi.useFakeTimers()
    const task = tasks.createTask({ title: 'Fix the bug' })

    for (let step = 0; step < 10; step += 1) {
      touching.touch(task.id)
      vi.advanceTimersByTime(50)
    }
    expect(sent).toEqual([])
    vi.advanceTimersByTime(DIRTY_DEBOUNCE_MS - 500)
    expect(sent).toEqual([{ kind: 'workspace.dirty', taskId: task.id }])
  })

  it('tells a few changes in a row soon, as when a shell starts and runs a command', () => {
    vi.useFakeTimers()
    const task = tasks.createTask({ title: 'Fix the bug' })
    const dirty = () => sent.filter((message) => message.kind === 'workspace.dirty').length

    for (let change = 1; change <= DIRTY_BURST; change += 1) {
      touching.touch(task.id)
      vi.advanceTimersByTime(DIRTY_DEBOUNCE_MS)
      expect(dirty()).toBe(change)
      vi.advanceTimersByTime(1000)
    }
  })

  it('tells once per interval while changes go on, and once after the last', () => {
    vi.useFakeTimers()
    const task = tasks.createTask({ title: 'Fix the bug' })
    const dirty = () => sent.filter((message) => message.kind === 'workspace.dirty').length

    touching.touch(task.id)
    vi.advanceTimersByTime(DIRTY_DEBOUNCE_MS)
    expect(dirty()).toBe(1)

    // A busy agent: a change every second for 25 seconds. The first ones are told soon, then one
    // per interval.
    for (let second = 0; second < 25; second += 1) {
      touching.touch(task.id)
      vi.advanceTimersByTime(1000)
    }
    expect(dirty()).toBe(DIRTY_BURST + 2)

    // The changes after the last message are told once, at the end of the interval.
    vi.advanceTimersByTime(DIRTY_MIN_INTERVAL_MS)
    expect(dirty()).toBe(DIRTY_BURST + 3)
    vi.advanceTimersByTime(DIRTY_MIN_INTERVAL_MS * (DIRTY_BURST + 1))
    expect(dirty()).toBe(DIRTY_BURST + 3)

    // After a quiet time, the first change is told soon again.
    touching.touch(task.id)
    vi.advanceTimersByTime(DIRTY_DEBOUNCE_MS)
    expect(dirty()).toBe(DIRTY_BURST + 4)
  })

  it('tells about each task apart', () => {
    vi.useFakeTimers()
    const first = tasks.createTask({ title: 'First' })
    const second = tasks.createTask({ title: 'Second' })

    touching.touch(first.id)
    touching.touch(second.id)
    touching.touch(first.id)
    vi.advanceTimersByTime(DIRTY_DEBOUNCE_MS)

    expect(sent).toEqual([
      { kind: 'workspace.dirty', taskId: first.id },
      { kind: 'workspace.dirty', taskId: second.id },
    ])
  })

  it('says nothing about a task deleted in the meantime', () => {
    vi.useFakeTimers()
    const task = tasks.createTask({ title: 'Fix the bug' })

    touching.touch(task.id)
    tasks.deleteTask(task.id)
    vi.advanceTimersByTime(DIRTY_DEBOUNCE_MS)

    expect(sent.filter((message) => message.kind === 'workspace.dirty')).toEqual([])
  })

  it('says nothing about a task deleted while it waited for the next interval', () => {
    vi.useFakeTimers()
    const task = tasks.createTask({ title: 'Fix the bug' })
    for (let change = 0; change < DIRTY_BURST; change += 1) {
      touching.touch(task.id)
      vi.advanceTimersByTime(DIRTY_DEBOUNCE_MS)
    }

    touching.touch(task.id)
    tasks.deleteTask(task.id)
    vi.advanceTimersByTime(DIRTY_MIN_INTERVAL_MS * 2)

    expect(sent.filter((message) => message.kind === 'workspace.dirty')).toHaveLength(DIRTY_BURST)
  })

  it('follows each change made here, such as a commit', async () => {
    const task = tasks.createTask({ title: 'Fix the bug' })
    const created = await touching.create(task.id)
    writeFileSync(join(created.workspace ?? '', 'notes.txt'), 'a\n')
    const touch = vi.spyOn(touching, 'touch')

    await touching.commit(task.id, 'Add notes')

    expect(touch).toHaveBeenCalledWith(task.id)
  })
})

describe('onCreated', () => {
  it('is told about a new workspace, and its failure does not undo the workspace', async () => {
    const seen: string[] = []
    const watched = new WorkspaceService({
      dataDir,
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
    expect(await workspaces.branches()).toEqual({
      branches: ['main', 'release'],
      current: 'main',
      defaultBranch: 'main',
    })
  })

  it('starts from the default branch when none is given, not from the checkout', async () => {
    runGit(repo.dir, 'checkout', '-b', 'feature')
    runGit(repo.dir, 'commit', '--allow-empty', '-m', 'on feature')

    const created = await workspaces.create(tasks.createTask({ title: 'Fix the bug' }).id)

    expect(runGit(created.workspace ?? '', 'log', '-1', '--format=%s').trim()).toBe('init')
  })

  // The picked branch was not pulled: the task starts from its remote copy, fetched now.
  it(
    'starts a picked branch from its newest remote commit',
    async () => {
      const origin = addBareOrigin(repo)
      runGit(repo.dir, 'branch', 'release')
      runGit(repo.dir, 'push', '-u', 'origin', 'release')
      const other = join(repo.root, 'other')
      runGit(repo.root, 'clone', '--branch', 'release', origin, other)
      runGit(other, 'commit', '--allow-empty', '-m', 'newer')
      runGit(other, 'push', 'origin', 'release')
      const newer = runGit(other, 'rev-parse', 'HEAD').trim()
      const local = runGit(repo.dir, 'rev-parse', 'release').trim()

      const created = await workspaces.create(tasks.createTask({ title: 'Fresh' }).id, 'release')

      expect(runGit(created.workspace ?? '', 'rev-parse', 'HEAD').trim()).toBe(newer)
      expect(runGit(repo.dir, 'rev-parse', 'release').trim()).toBe(local)
      expect(() => runGit(created.workspace ?? '', 'rev-parse', '@{upstream}')).toThrow()
    },
    GIT_TEST_MS,
  )

  it(
    'starts a picked branch from its local commits that are not pushed, even offline',
    async () => {
      const origin = addBareOrigin(repo)
      runGit(repo.dir, 'checkout', '-b', 'release')
      runGit(repo.dir, 'push', '-u', 'origin', 'release')
      runGit(repo.dir, 'commit', '--allow-empty', '-m', 'local only')
      runGit(repo.dir, 'checkout', 'main')
      const local = runGit(repo.dir, 'rev-parse', 'release').trim()

      const first = await workspaces.create(tasks.createTask({ title: 'Ahead' }).id, 'release')
      expect(runGit(first.workspace ?? '', 'rev-parse', 'HEAD').trim()).toBe(local)

      rmSync(origin, { recursive: true, force: true })
      const second = await workspaces.create(tasks.createTask({ title: 'Offline' }).id, 'release')
      expect(runGit(second.workspace ?? '', 'rev-parse', 'HEAD').trim()).toBe(local)
    },
    GIT_TEST_MS,
  )

  it('starts a workspace from the branch it is given', async () => {
    runGit(repo.dir, 'checkout', '-b', 'release')
    runGit(repo.dir, 'commit', '--allow-empty', '-m', 'on release')
    runGit(repo.dir, 'checkout', 'main')
    const task = tasks.createTask({ title: 'Fix the bug' })

    const created = await workspaces.create(task.id, 'release')

    expect(runGit(created.workspace ?? '', 'log', '-1', '--format=%s').trim()).toBe('on release')
  })

  it('starts a subtask from its parent branch, unless a base is given', async () => {
    const parent = await workspaces.create(tasks.createTask({ title: 'Parent' }).id)
    runGit(parent.workspace ?? '', 'commit', '--allow-empty', '-m', 'on parent')
    runGit(repo.dir, 'branch', 'release')

    const child = await workspaces.create(
      tasks.createTask({ title: 'Part A', parentId: parent.id }).id,
    )
    const picked = await workspaces.create(
      tasks.createTask({ title: 'Part B', parentId: parent.id }).id,
      'release',
    )

    expect(runGit(child.workspace ?? '', 'log', '-1', '--format=%s').trim()).toBe('on parent')
    expect(runGit(picked.workspace ?? '', 'log', '-1', '--format=%s').trim()).toBe('init')
  })

  it('starts a subtask from the default branch when its parent has no branch', async () => {
    const parent = tasks.createTask({ title: 'Parent' })
    const child = await workspaces.create(
      tasks.createTask({ title: 'Part A', parentId: parent.id }).id,
    )
    expect(runGit(child.workspace ?? '', 'log', '-1', '--format=%s').trim()).toBe('init')
  })

  it('refuses a branch that does not exist', async () => {
    const task = tasks.createTask({ title: 'Fix the bug' })
    await expect(workspaces.create(task.id, 'nope')).rejects.toThrow(WorkspaceConflictError)
  })
})
