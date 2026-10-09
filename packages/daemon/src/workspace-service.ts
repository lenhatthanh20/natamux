import { join } from 'node:path'
import type { ChangeEntry, DiffStat, FileDiff, GitGraph, Task } from '@helmsman/core'
import {
  addWorktree,
  commitAll,
  createPullRequest,
  currentBranch,
  deleteMergedBranch,
  listBranches,
  listChanges,
  pushBranch,
  type RunGh,
  readDiffStat,
  readFileDiff,
  readGitGraph,
  removeWorktree,
  runGh,
} from './git.js'
import type { RepoService } from './repo-service.js'
import type { TaskService } from './task-service.js'

export class WorkspaceConflictError extends Error {
  constructor(message: string) {
    super(message)
    this.name = 'WorkspaceConflictError'
  }
}

export class FileNotChangedError extends Error {
  constructor(path: string) {
    super(`${path} is not one of the changed files of the workspace`)
    this.name = 'FileNotChangedError'
  }
}

type WorkspaceServiceOptions = {
  tasks: TaskService
  repoDir: string
  worktreesDir: string
  // Other repositories; a task names its own in `repo`. Without it, every task uses repoDir.
  repos?: RepoService
  gh?: RunGh
  // Called after a workspace is made, for example to run its setup script.
  onCreated?: (task: Task) => void
  // The body of a task's pull request. Default: the task's notes.
  pullRequestBody?: (task: Task) => Promise<string>
}

export type RemoveWorkspaceResult = {
  task: Task
  branchDeleted: boolean
}

const MAX_SLUG_LENGTH = 40

export function slugify(text: string): string {
  const slug = text
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .slice(0, MAX_SLUG_LENGTH)
    .replace(/-+$/g, '')
  return slug === '' ? 'task' : slug
}

export function branchNameFor(task: Pick<Task, 'id' | 'title'>): string {
  return `helmsman/${task.id.toLowerCase()}-${slugify(task.title)}`
}

export class WorkspaceService {
  private readonly tasks: TaskService
  private readonly repoDir: string
  private readonly worktreesDir: string
  private readonly repos: RepoService | undefined
  private readonly gh: RunGh
  private readonly onCreated: (task: Task) => void
  private readonly pullRequestBody: (task: Task) => Promise<string>
  private readonly busy = new Set<string>()

  constructor(options: WorkspaceServiceOptions) {
    this.tasks = options.tasks
    this.repoDir = options.repoDir
    this.repos = options.repos
    this.worktreesDir = options.worktreesDir
    this.gh = options.gh ?? runGh
    this.onCreated = options.onCreated ?? (() => undefined)
    this.pullRequestBody =
      options.pullRequestBody ??
      (async (task) => task.description.trim() || `Task ${task.id} in Helmsman.`)
  }

  // The folder name is the task id only, to keep paths short.
  worktreePathFor(taskId: string): string {
    return join(this.worktreesDir, taskId)
  }

  // The repository folder of a task, or of a repository id (null: the default one).
  repoDirOf(repo: string | null): string {
    return this.repos ? this.repos.pathOf(repo) : this.repoDir
  }

  // The branches a workspace in that repository can start from.
  async branches(
    repo: string | null = null,
  ): Promise<{ branches: string[]; current: string | null }> {
    return listBranches(this.repoDirOf(repo))
  }

  // Makes the worktree from `base` when given (a local branch), or from the checkout's HEAD.
  async create(taskId: string, base?: string): Promise<Task> {
    return this.exclusive(taskId, async () => {
      const task = this.tasks.getTask(taskId)
      if (task.workspace !== null) {
        throw new WorkspaceConflictError(`${taskId} already has a workspace at ${task.workspace}`)
      }
      if (task.state === 'done' || task.state === 'cancelled') {
        throw new WorkspaceConflictError(`${taskId} is ${task.state}, so it gets no workspace`)
      }

      const repoDir = this.repoDirOf(task.repo)
      if (base !== undefined && !(await listBranches(repoDir)).branches.includes(base)) {
        throw new WorkspaceConflictError(`There is no local branch named ${base}`)
      }
      const branch = branchNameFor(task)
      const worktreePath = this.worktreePathFor(taskId)
      await addWorktree(repoDir, worktreePath, branch, base)

      let created: Task
      try {
        created = this.tasks.recordWorkspace(taskId, { workspace: worktreePath, branch })
      } catch (error) {
        await removeWorktree(repoDir, worktreePath).catch(() => undefined)
        throw error
      }
      // The workspace is made either way; a failing setup shows up in its terminal.
      try {
        this.onCreated(created)
      } catch {
        // Nothing to undo.
      }
      return created
    })
  }

  // Removes the worktree folder. The branch goes too when it is fully merged, otherwise it stays.
  async remove(taskId: string, options: { deleteBranch: boolean }): Promise<RemoveWorkspaceResult> {
    return this.exclusive(taskId, async () => {
      const task = this.tasks.getTask(taskId)
      if (task.workspace === null) {
        throw new WorkspaceConflictError(`${taskId} has no workspace`)
      }

      const repoDir = this.repoDirOf(task.repo)
      await removeWorktree(repoDir, task.workspace)

      const branchDeleted =
        options.deleteBranch && task.branch !== null
          ? await deleteMergedBranch(repoDir, task.branch)
          : false
      const updated = this.tasks.recordWorkspace(taskId, {
        workspace: null,
        branch: branchDeleted ? null : task.branch,
      })
      return { task: updated, branchDeleted }
    })
  }

  // The files of the workspace that differ from its last commit.
  async changes(taskId: string): Promise<ChangeEntry[]> {
    return listChanges(this.workspaceOf(taskId))
  }

  // Only a file that is in the list of changes can be read: this is what keeps the request from
  // reaching any other file.
  async fileDiff(taskId: string, path: string): Promise<FileDiff> {
    const workspace = this.workspaceOf(taskId)
    const change = (await listChanges(workspace)).find((entry) => entry.path === path)
    if (!change) {
      throw new FileNotChangedError(path)
    }
    return readFileDiff(workspace, change)
  }

  // The size of the work in each workspace. A workspace that git cannot read is left out.
  async diffStats(): Promise<Record<string, DiffStat>> {
    const bases = new Map<string | null, Promise<string | null>>()
    const baseOf = (repo: string | null) => {
      const known = bases.get(repo) ?? currentBranch(this.repoDirOf(repo))
      bases.set(repo, known)
      return known
    }
    const withWorkspace = this.tasks.listTasks().filter((task) => task.workspace !== null)
    const entries = await Promise.all(
      withWorkspace.map(async (task) => {
        try {
          const base = await baseOf(task.repo)
          return [[task.id, await readDiffStat(task.workspace ?? '', base)] as const]
        } catch {
          return []
        }
      }),
    )
    return Object.fromEntries(entries.flat())
  }

  // The history of the task branch next to the branch that the main checkout is on.
  async graph(taskId: string): Promise<GitGraph> {
    const workspace = this.workspaceOf(taskId)
    const task = this.tasks.getTask(taskId)
    return readGitGraph(workspace, await currentBranch(this.repoDirOf(task.repo)))
  }

  // Commits every change of the workspace on the task branch.
  async commit(taskId: string, message: string): Promise<{ task: Task; hash: string }> {
    return this.exclusive(taskId, async () => {
      const { hash, subject } = await commitAll(this.workspaceOf(taskId), message)
      const task = this.tasks.recordGit(taskId, 'git.committed', { hash, subject })
      return { task, hash }
    })
  }

  // Pushes the task branch to `origin` and makes it the upstream of the branch.
  async push(taskId: string): Promise<Task> {
    return this.exclusive(taskId, async () => this.pushNow(taskId))
  }

  // Pushes, then opens a pull request into the default branch. A task that has one keeps it.
  async openPullRequest(taskId: string): Promise<Task> {
    return this.exclusive(taskId, async () => {
      const task = this.tasks.getTask(taskId)
      if (task.pr !== null) {
        throw new WorkspaceConflictError(`${taskId} has a pull request already: ${task.pr}`)
      }
      const pushed = await this.pushNow(taskId)
      const url = await createPullRequest(
        this.workspaceOf(taskId),
        {
          branch: pushed.branch ?? '',
          title: pushed.title,
          body: await this.pullRequestBody(pushed),
        },
        this.gh,
      )
      return this.tasks.recordGit(taskId, 'pr.opened', { url })
    })
  }

  private async pushNow(taskId: string): Promise<Task> {
    const workspace = this.workspaceOf(taskId)
    const { branch } = this.tasks.getTask(taskId)
    if (branch === null) {
      throw new WorkspaceConflictError(`${taskId} has no branch`)
    }
    await pushBranch(workspace, branch)
    return this.tasks.recordGit(taskId, 'git.pushed', { branch })
  }

  // The folder of the task's workspace. Throws WorkspaceConflictError when it has none.
  workspaceOf(taskId: string): string {
    const task = this.tasks.getTask(taskId)
    if (task.workspace === null) {
      throw new WorkspaceConflictError(`${taskId} has no workspace`)
    }
    return task.workspace
  }

  private async exclusive<T>(taskId: string, work: () => Promise<T>): Promise<T> {
    if (this.busy.has(taskId)) {
      throw new WorkspaceConflictError(`Another workspace change for ${taskId} is still running`)
    }
    this.busy.add(taskId)
    try {
      return await work()
    } finally {
      this.busy.delete(taskId)
    }
  }
}
