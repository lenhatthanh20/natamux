import { join } from 'node:path'
import type {
  ChangeEntry,
  CommitMessageDraft,
  DiffStat,
  FileDiff,
  GitGraph,
  ImageSide,
  Task,
  TreeEntry,
  WorkspaceFile,
} from '@natamux/core'
import { DEFAULT_PROGRAMS, type LaunchableAgent } from '../agents/agent-launch.js'
import { type BranchList, listBranches } from '../git/branches.js'
import { listChanges } from '../git/changes.js'
import { type RunAgentOnce, runAgentOnce } from '../git/commit-drafter.js'
import { type RunGh, runGh } from '../git/gh.js'
import { fetchUpstream, startRef } from '../git/graph.js'
import { addWorktree, hasSubmodules, removeWorktree, worktreeState } from '../git/worktrees.js'
import type { RepoService } from '../projects/repo-service.js'
import type { EventBus } from '../shared/event-bus.js'
import { archiveStageFiles } from '../stages/stage-files.js'
import type { TaskService } from '../tasks/task-service.js'
import { branchNameFor, deleteFinishedBranch } from './branch-name.js'
import { DiffStats } from './diff-stats.js'
import { DirtyTracker } from './dirty-tracker.js'
import { WorkspaceChanges } from './workspace-changes.js'
import { type RemoveWorkspaceResult, WorkspaceConflictError } from './workspace-errors.js'
import { WorkspacePublish } from './workspace-publish.js'

type WorkspaceServiceOptions = {
  // Where the stage files of a task are archived.
  dataDir: string
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
  // Ends the shells of a task, before its worktree is removed. A promise it returns ends when
  // the shells have ended.
  closeTerminals?: (taskId: string) => unknown
  // The command of each agent, for the run that drafts a commit message. Default: its usual name.
  programOf?: (agent: LaunchableAgent) => string
  runAgentOnce?: RunAgentOnce
  // Where a failed draft is written down. Default: console.warn.
  log?: (message: string) => void
  now?: () => number
  // Dashboards hear when the files of a workspace may have changed.
  bus?: EventBus
}

export class WorkspaceService {
  private readonly dataDir: string
  private readonly tasks: TaskService
  private readonly repoDir: string | undefined
  private readonly worktreesDir: string
  private readonly repos: RepoService | undefined
  private readonly onCreated: (task: Task) => void
  private readonly closeTerminals: (taskId: string) => unknown
  private readonly busy = new Set<string>()
  private readonly stats: DiffStats
  private readonly reads: WorkspaceChanges
  private readonly dirty: DirtyTracker
  private readonly publishing: WorkspacePublish

  constructor(options: WorkspaceServiceOptions) {
    this.dataDir = options.dataDir
    this.tasks = options.tasks
    this.repoDir = options.repoDir
    this.repos = options.repos
    this.worktreesDir = options.worktreesDir
    this.onCreated = options.onCreated ?? (() => undefined)
    this.closeTerminals = options.closeTerminals ?? (() => undefined)
    const now = options.now ?? Date.now
    // The parts call back through the service, so a spy on touch sees their calls too.
    const workspaceOf = (taskId: string) => this.workspaceOf(taskId)
    const repoDirOf = (repo: string | null) => this.repoDirOf(repo)
    this.stats = new DiffStats(this.tasks, repoDirOf, now)
    this.reads = new WorkspaceChanges({
      tasks: this.tasks,
      now,
      workspaceOf,
      repoDirOf,
      touch: (taskId) => this.touch(taskId),
    })
    this.dirty = new DirtyTracker(this.tasks, options.bus)
    this.publishing = new WorkspacePublish({
      tasks: this.tasks,
      gh: options.gh ?? runGh,
      pullRequestBody:
        options.pullRequestBody ??
        (async (task) => task.description.trim() || `Task ${task.id} in Natamux.`),
      programOf: options.programOf ?? ((agent) => DEFAULT_PROGRAMS[agent]),
      runAgentOnce: options.runAgentOnce ?? runAgentOnce,
      log: options.log ?? ((message) => console.warn(message)),
      workspaceOf,
      exclusive: (taskId, work) => this.exclusive(taskId, work),
    })
  }

  // The folder name is the task id only, to keep paths short.
  worktreePathFor(task: Pick<Task, 'id' | 'repo'>): string {
    return join(this.repos?.worktreesDirOf(task.repo) ?? this.worktreesDir, task.id)
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
  async branches(repo: string | null = null): Promise<BranchList> {
    return listBranches(this.repoDirOf(repo))
  }

  // Makes the worktree from `base` when given (a local branch), or else from the parent's branch
  // for a subtask, or else from the default branch,
  // as its remote has it now. The local branch when it has commits that are not pushed, or has no
  // remote. HEAD when the repository has no branch to start from.
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
      const { branches, defaultBranch } = await listBranches(repoDir)
      if (base !== undefined && !branches.includes(base)) {
        throw new WorkspaceConflictError(`There is no local branch named ${base}`)
      }
      // A workspace removed with its branch kept leaves the task's work there: go on with it.
      const kept = task.branch !== null && branches.includes(task.branch) ? task.branch : null
      const worktreePath = this.worktreePathFor(task)
      const branch = kept ?? branchNameFor(task)
      if (kept === null) {
        // A subtask's work is merged into its parent's branch, so it starts there.
        const parent = task.parentId === null ? null : this.tasks.getTask(task.parentId).branch
        const from = base ?? (parent !== null && branches.includes(parent) ? parent : defaultBranch)
        if (from === null) {
          await addWorktree(repoDir, worktreePath, branch)
        } else {
          await fetchUpstream(repoDir, from)
          await addWorktree(repoDir, worktreePath, branch, (await startRef(repoDir, from)) ?? from)
        }
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
      let submodules = false
      if (state === 'present') {
        // git refuses a worktree with changes that are not committed. Refuse it here, before the
        // agent and the shells of the task are ended for nothing. A status that git cannot read
        // proves nothing, so its error stops the removal too.
        const changes = await listChanges(task.workspace)
        if (changes.length > 0) {
          const count =
            changes.length === 1
              ? '1 change that is not committed. Commit or discard it'
              : `${changes.length} changes that are not committed. Commit or discard them`
          throw new WorkspaceConflictError(`${taskId} has ${count} first`)
        }
        // git also refuses a clean worktree with submodules checked out. The status above counts
        // changes inside submodules too, so forcing loses no work.
        submodules = await hasSubmodules(task.workspace)
      }
      // A shell that runs in the folder would keep files open while git removes it.
      await this.closeTerminals(taskId)
      // A stale entry keeps git from deleting the branch or adding the worktree again.
      if (state !== 'missing') {
        try {
          // After the shells ended, so the agent writes no stage file after the copy.
          archiveStageFiles(this.dataDir, task)
          await removeWorktree(repoDir, task.workspace, submodules)
        } catch (error) {
          // The agent ended with its shell. Left running, the task would wait for it forever.
          this.tasks.stopAgents(taskId)
          throw error
        }
      }

      const branchDeleted =
        options.deleteBranch && task.branch !== null
          ? await deleteFinishedBranch(repoDir, task.branch, task.prStatus)
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

  // The file reads (see WorkspaceChanges).
  changes(taskId: string, fresh = false): Promise<ChangeEntry[]> {
    return this.reads.changes(taskId, fresh)
  }

  fileDiff(taskId: string, path: string): Promise<FileDiff> {
    return this.reads.fileDiff(taskId, path)
  }

  fileImage(
    taskId: string,
    path: string,
    side: ImageSide,
  ): Promise<{ type: string; bytes: Buffer }> {
    return this.reads.fileImage(taskId, path, side)
  }

  tree(taskId: string, path: string): Promise<TreeEntry[]> {
    return this.reads.tree(taskId, path)
  }

  readFile(taskId: string, path: string): Promise<WorkspaceFile> {
    return this.reads.readFile(taskId, path)
  }

  writeFile(
    taskId: string,
    path: string,
    text: string,
    version: string,
  ): Promise<{ path: string; version: string }> {
    return this.reads.writeFile(taskId, path, text, version)
  }

  graph(taskId: string, fresh = false): Promise<GitGraph> {
    return this.reads.graph(taskId, fresh)
  }

  // The size of the work in each workspace. A workspace that git cannot read is left out.
  diffStats(): Promise<Record<string, DiffStat>> {
    return this.stats.get()
  }

  // The workspace of the task may have changed: by its agent, in one of its shells, or here. The
  // next reads of git see it, and dashboards hear of it (see DIRTY_DEBOUNCE_MS).
  touch(taskId: string): void {
    this.stats.touch(taskId)
    this.reads.forget(taskId)
    this.dirty.touch(taskId)
  }

  // Commit, push and pull request (see WorkspacePublish).
  commit(taskId: string, message: string): Promise<{ task: Task; hash: string }> {
    return this.publishing.commit(taskId, message)
  }

  draftCommitMessage(taskId: string): Promise<CommitMessageDraft> {
    return this.publishing.draftCommitMessage(taskId)
  }

  push(taskId: string): Promise<Task> {
    return this.publishing.push(taskId)
  }

  openPullRequest(taskId: string): Promise<Task> {
    return this.publishing.openPullRequest(taskId)
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
      // Every change of a workspace runs here, so the next read sees it, even after a failure.
      this.touch(taskId)
    }
  }
}
