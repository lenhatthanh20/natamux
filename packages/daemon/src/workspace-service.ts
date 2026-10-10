import { join } from 'node:path'
import type { ChangeEntry, DiffStat, FileDiff, GitGraph, ImageSide, Task } from '@natamux/core'
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
  readFileImage,
  readGitGraph,
  removeWorktree,
  runGh,
  worktreeState,
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
  // The repository of every task, when there is no `repos`.
  repoDir?: string
  worktreesDir: string
  // The repositories; a task names its own in `repo`.
  repos?: RepoService
  gh?: RunGh
  // Called after a workspace is made, for example to run its setup script.
  onCreated?: (task: Task) => void
  // The body of a task's pull request. Default: the task's notes.
  pullRequestBody?: (task: Task) => Promise<string>
  // Ends the shells of a task, before its worktree is removed.
  closeTerminals?: (taskId: string) => void
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
  return `natamux/${task.id.toLowerCase()}-${slugify(task.title)}`
}

export class WorkspaceService {
  private readonly tasks: TaskService
  private readonly repoDir: string | undefined
  private readonly worktreesDir: string
  private readonly repos: RepoService | undefined
  private readonly gh: RunGh
  private readonly onCreated: (task: Task) => void
  private readonly pullRequestBody: (task: Task) => Promise<string>
  private readonly closeTerminals: (taskId: string) => void
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
      (async (task) => task.description.trim() || `Task ${task.id} in Natamux.`)
    this.closeTerminals = options.closeTerminals ?? (() => undefined)
  }

  // The folder name is the task id only, to keep paths short.
  worktreePathFor(taskId: string): string {
    return join(this.worktreesDir, taskId)
  }

  // The repository folder of a task, or of a repository id (null: the default one).
  repoDirOf(repo: string | null): string {
    if (this.repos !== undefined) {
      return this.repos.pathOf(repo)
    }
    if (this.repoDir === undefined) {
      throw new Error('WorkspaceService needs repos or repoDir')
    }
    return this.repoDir
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
      const { branches } = await listBranches(repoDir)
      if (base !== undefined && !branches.includes(base)) {
        throw new WorkspaceConflictError(`There is no local branch named ${base}`)
      }
      // A workspace removed with its branch kept leaves the task's work there: go on with it.
      const kept = task.branch !== null && branches.includes(task.branch) ? task.branch : null
      const worktreePath = this.worktreePathFor(taskId)
      const branch = kept ?? branchNameFor(task)
      if (kept === null) {
        await addWorktree(repoDir, worktreePath, branch, base)
      } else {
        await addWorktree(repoDir, worktreePath, branch, undefined, true)
      }

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
  // A folder that was already deleted outside Natamux is only cleared from git and the task.
  async remove(taskId: string, options: { deleteBranch: boolean }): Promise<RemoveWorkspaceResult> {
    return this.exclusive(taskId, async () => {
      const task = this.tasks.getTask(taskId)
      if (task.workspace === null) {
        throw new WorkspaceConflictError(`${taskId} has no workspace`)
      }

      const repoDir = this.repoDirOf(task.repo)
      const state = await worktreeState(repoDir, task.workspace)
      // The folder may hold someone's own files, so it is never deleted.
      if (state === 'not-a-worktree') {
        throw new WorkspaceConflictError(
          `${task.workspace} is not a git worktree of this repository, so it is not removed`,
        )
      }
      if (state === 'present') {
        // git refuses a worktree with changes that are not committed. Refuse it here, before the
        // agent and the shells of the task are ended for nothing.
        const changes = await listChanges(task.workspace).catch(() => [])
        if (changes.length > 0) {
          const count = changes.length === 1 ? '1 change' : `${changes.length} changes`
          throw new WorkspaceConflictError(
            `${taskId} has ${count} that are not committed. Commit or discard them first`,
          )
        }
      }
      // A shell that runs in the folder would keep files open while git removes it.
      this.closeTerminals(taskId)
      // A stale entry keeps git from deleting the branch or adding the worktree again.
      if (state !== 'missing') {
        await removeWorktree(repoDir, task.workspace)
      }

      const branchDeleted =
        options.deleteBranch && task.branch !== null
          ? await deleteMergedBranch(repoDir, task.branch)
          : false
      this.tasks.recordWorkspace(taskId, {
        workspace: null,
        branch: branchDeleted ? null : task.branch,
      })
      // After the workspace is cleared: a hook of the ending agent may have moved the task while
      // git worked, and later hooks are ignored (see AgentHookService).
      return { task: this.tasks.stopAgents(taskId), branchDeleted }
    })
  }

  // The files of the workspace that differ from its last commit, with the lines of each.
  async changes(taskId: string): Promise<ChangeEntry[]> {
    return listChanges(this.workspaceOf(taskId), { counts: true })
  }

  // Only a file that is in the list of changes can be read: this is what keeps the request from
  // reaching any other file.
  async fileDiff(taskId: string, path: string): Promise<FileDiff> {
    const workspace = this.workspaceOf(taskId)
    return readFileDiff(workspace, await this.changedFile(workspace, path))
  }

  // One side of a changed image, with the same guard as fileDiff.
  async fileImage(
    taskId: string,
    path: string,
    side: ImageSide,
  ): Promise<{ type: string; bytes: Buffer }> {
    const workspace = this.workspaceOf(taskId)
    return readFileImage(workspace, await this.changedFile(workspace, path), side)
  }

  private async changedFile(workspace: string, path: string): Promise<ChangeEntry> {
    const change = (await listChanges(workspace)).find((entry) => entry.path === path)
    if (!change) {
      throw new FileNotChangedError(path)
    }
    return change
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
