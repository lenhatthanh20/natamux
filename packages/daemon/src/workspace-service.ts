import { join } from 'node:path'
import type { ChangeEntry, FileDiff, Task } from '@helmsman/core'
import {
  addWorktree,
  deleteMergedBranch,
  listChanges,
  readFileDiff,
  removeWorktree,
} from './git.js'
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
  private readonly busy = new Set<string>()

  constructor(options: WorkspaceServiceOptions) {
    this.tasks = options.tasks
    this.repoDir = options.repoDir
    this.worktreesDir = options.worktreesDir
  }

  // The folder name is the task id only, to keep paths short.
  worktreePathFor(taskId: string): string {
    return join(this.worktreesDir, taskId)
  }

  async create(taskId: string): Promise<Task> {
    return this.exclusive(taskId, async () => {
      const task = this.tasks.getTask(taskId)
      if (task.workspace !== null) {
        throw new WorkspaceConflictError(`${taskId} already has a workspace at ${task.workspace}`)
      }
      if (task.state === 'done' || task.state === 'cancelled') {
        throw new WorkspaceConflictError(`${taskId} is ${task.state}, so it gets no workspace`)
      }

      const branch = branchNameFor(task)
      const worktreePath = this.worktreePathFor(taskId)
      await addWorktree(this.repoDir, worktreePath, branch)

      try {
        return this.tasks.recordWorkspace(taskId, { workspace: worktreePath, branch })
      } catch (error) {
        await removeWorktree(this.repoDir, worktreePath).catch(() => undefined)
        throw error
      }
    })
  }

  // Removes the worktree folder. The branch goes too when it is fully merged, otherwise it stays.
  async remove(taskId: string, options: { deleteBranch: boolean }): Promise<RemoveWorkspaceResult> {
    return this.exclusive(taskId, async () => {
      const task = this.tasks.getTask(taskId)
      if (task.workspace === null) {
        throw new WorkspaceConflictError(`${taskId} has no workspace`)
      }

      await removeWorktree(this.repoDir, task.workspace)

      const branchDeleted =
        options.deleteBranch && task.branch !== null
          ? await deleteMergedBranch(this.repoDir, task.branch)
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

  private workspaceOf(taskId: string): string {
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
