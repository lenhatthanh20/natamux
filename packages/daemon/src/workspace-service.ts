import { join } from 'node:path'
import type {
  ChangeEntry,
  CommitMessageDraft,
  DiffStat,
  FileDiff,
  GitGraph,
  ImageSide,
  Task,
} from '@natamux/core'
import { DEFAULT_PROGRAMS, isLaunchable, type LaunchableAgent } from './agent-launch.js'
import {
  draftCommitMessage,
  MAX_DIFF_CHARS,
  type RunAgentOnce,
  runAgentOnce,
} from './commit-drafter.js'
import type { EventBus } from './event-bus.js'
import {
  addWorktree,
  type BranchList,
  commitAll,
  createPullRequest,
  currentBranch,
  deleteMergedBranch,
  fetchUpstream,
  hasSubmodules,
  listBranches,
  listChanges,
  pushBranch,
  type RunGh,
  readCommitDiff,
  readDiffStat,
  readFileDiff,
  readFileImage,
  readGitGraph,
  removeWorktree,
  runGh,
  startRef,
  worktreeState,
} from './git.js'
import { mapLimit } from './map-limit.js'
import type { RepoService } from './repo-service.js'
import type { TaskService } from './task-service.js'
import { TtlCache } from './ttl-cache.js'

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

export type RemoveWorkspaceResult = {
  task: Task
  branchDeleted: boolean
}

const MAX_SLUG_LENGTH = 40
const DIFF_STAT_READS = 4
// Dashboards ask for the changes, the graph and the diff stats on timers. Within this time they
// share one read, so git runs less often. A change made through this service is seen at once.
export const GIT_READ_MS = 5000
// The diff stat of a workspace is read again when the workspace may have changed (see touch). A
// change that no hook, shell or commit here saw, such as one in another editor, shows after this.
export const DIFF_STATS_MAX_AGE_MS = 5 * 60_000
// Hooks and output come in bursts: dashboards hear of a change in a task this long after it. Each
// message makes them read git again. A few in a row go out soon, so a user's command in a new shell
// shows at once; after that, one per DIRTY_MIN_INTERVAL_MS while changes go on, so a busy agent
// costs no more than the old 10 s poll.
export const DIRTY_DEBOUNCE_MS = 750
export const DIRTY_BURST = 3
export const DIRTY_MIN_INTERVAL_MS = 10_000

// The workspace.dirty messages of one task. `credits`: how many may go out soon; one comes back
// per DIRTY_MIN_INTERVAL_MS, up to DIRTY_BURST. `again`: a touch came that no message told yet.
type DirtyState = {
  credits: number
  again: boolean
  tell?: NodeJS.Timeout
  refill?: NodeJS.Timeout
}

// What the diff of a workspace counts from: where a new workspace starts, by name.
async function baseForDiff(repoDir: string): Promise<string | null> {
  const branch = await currentBranch(repoDir)
  return branch === null ? null : ((await startRef(repoDir, branch)) ?? branch)
}

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
  private readonly closeTerminals: (taskId: string) => unknown
  private readonly programOf: (agent: LaunchableAgent) => string
  private readonly runAgentOnce: RunAgentOnce
  private readonly log: (message: string) => void
  private readonly busy = new Set<string>()
  private readonly drafts = new Map<string, Promise<CommitMessageDraft>>()
  private readonly diffStatsReads: TtlCache<Record<string, DiffStat>>
  private readonly changesReads: TtlCache<ChangeEntry[]>
  private readonly graphReads: TtlCache<GitGraph>
  private readonly now: () => number
  private readonly bus: EventBus | undefined
  // The last diff stat of each workspace, and the tasks whose workspace changed since.
  private readonly stats = new Map<string, { stat: DiffStat; readAt: number; round: number }>()
  private readonly dirtyStats = new Set<string>()
  private statsRound = 0
  private readonly dirty = new Map<string, DirtyState>()

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
    this.programOf = options.programOf ?? ((agent) => DEFAULT_PROGRAMS[agent])
    this.runAgentOnce = options.runAgentOnce ?? runAgentOnce
    this.log = options.log ?? ((message) => console.warn(message))
    this.now = options.now ?? Date.now
    this.bus = options.bus
    this.diffStatsReads = new TtlCache(GIT_READ_MS, this.now)
    this.changesReads = new TtlCache(GIT_READ_MS, this.now)
    this.graphReads = new TtlCache(GIT_READ_MS, this.now)
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
  async branches(repo: string | null = null): Promise<BranchList> {
    return listBranches(this.repoDirOf(repo))
  }

  // Makes the worktree from `base` when given (a local branch), or else from the default branch,
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
      const worktreePath = this.worktreePathFor(taskId)
      const branch = kept ?? branchNameFor(task)
      if (kept === null) {
        const from = base ?? defaultBranch
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
          const count = changes.length === 1 ? '1 change' : `${changes.length} changes`
          throw new WorkspaceConflictError(
            `${taskId} has ${count} that are not committed. Commit or discard them first`,
          )
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
          await removeWorktree(repoDir, task.workspace, submodules)
        } catch (error) {
          // The agent ended with its shell. Left running, the task would wait for it forever.
          this.tasks.stopAgents(taskId)
          throw error
        }
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

  // The files of the workspace that differ from its last commit, with the lines of each. `fresh`
  // reads git now, and the answer replaces the kept one.
  async changes(taskId: string, fresh = false): Promise<ChangeEntry[]> {
    const workspace = this.workspaceOf(taskId)
    if (fresh) {
      this.changesReads.forget(taskId)
    }
    return this.changesReads.get(taskId, () => listChanges(workspace, { counts: true }))
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
  diffStats(): Promise<Record<string, DiffStat>> {
    return this.diffStatsReads.get('', () => this.readDiffStats())
  }

  private async readDiffStats(): Promise<Record<string, DiffStat>> {
    const bases = new Map<string | null, Promise<string | null>>()
    const baseOf = (repo: string | null) => {
      // Where a new workspace starts, so the commits the checkout did not pull do not count.
      const known = bases.get(repo) ?? baseForDiff(this.repoDirOf(repo))
      bases.set(repo, known)
      return known
    }
    const withWorkspace = this.tasks.listTasks().filter((task) => task.workspace !== null)
    const readAt = this.now()
    this.statsRound += 1
    const round = this.statsRound
    // A touch during this read starts another one. When that newer read ends first, this one must
    // not put back its older stat.
    const newer = (taskId: string) => (this.stats.get(taskId)?.round ?? 0) > round
    // Two git commands per workspace: with many workspaces, all at once would load every core.
    const entries = await mapLimit(withWorkspace, DIFF_STAT_READS, async (task) => {
      const known = this.stats.get(task.id)
      if (
        known !== undefined &&
        !this.dirtyStats.has(task.id) &&
        readAt - known.readAt < DIFF_STATS_MAX_AGE_MS
      ) {
        return [[task.id, known.stat] as const]
      }
      // Before the read: a change while git runs marks the task again.
      this.dirtyStats.delete(task.id)
      try {
        const stat = await readDiffStat(task.workspace ?? '', await baseOf(task.repo))
        if (!newer(task.id)) {
          this.stats.set(task.id, { stat, readAt, round })
        }
        return [[task.id, stat] as const]
      } catch {
        if (!newer(task.id)) {
          this.stats.delete(task.id)
        }
        return []
      }
    })
    const ids = new Set(withWorkspace.map((task) => task.id))
    for (const id of this.stats.keys()) {
      if (!ids.has(id)) {
        this.stats.delete(id)
      }
    }
    return Object.fromEntries(entries.flat())
  }

  // The workspace of the task may have changed: by its agent, in one of its shells, or here. The
  // next reads of git see it, and dashboards hear of it (see DIRTY_DEBOUNCE_MS).
  touch(taskId: string): void {
    this.dirtyStats.add(taskId)
    // The diff stats of the other tasks still come from `stats`: only this one runs git again.
    this.diffStatsReads.forget()
    this.changesReads.forget(taskId)
    this.graphReads.forget(taskId)
    const bus = this.bus
    if (bus === undefined) {
      return
    }
    const state: DirtyState = this.dirty.get(taskId) ?? { credits: DIRTY_BURST, again: false }
    this.dirty.set(taskId, state)
    state.again = true
    this.planDirty(taskId, state, bus)
  }

  private planDirty(taskId: string, state: DirtyState, bus: EventBus): void {
    if (state.tell !== undefined || state.credits === 0 || !state.again) {
      return
    }
    state.tell = setTimeout(() => {
      state.tell = undefined
      if (!this.taskExists(taskId)) {
        clearTimeout(state.refill)
        this.dirty.delete(taskId)
        return
      }
      state.again = false
      state.credits -= 1
      bus.publish({ kind: 'workspace.dirty', taskId })
      this.refillDirty(taskId, state, bus)
    }, DIRTY_DEBOUNCE_MS)
    state.tell.unref()
  }

  private refillDirty(taskId: string, state: DirtyState, bus: EventBus): void {
    if (state.refill !== undefined) {
      return
    }
    state.refill = setTimeout(() => {
      state.refill = undefined
      state.credits += 1
      if (state.credits < DIRTY_BURST) {
        this.refillDirty(taskId, state, bus)
      } else if (state.tell === undefined && !state.again) {
        this.dirty.delete(taskId)
        return
      }
      this.planDirty(taskId, state, bus)
    }, DIRTY_MIN_INTERVAL_MS)
    state.refill.unref()
  }

  private taskExists(taskId: string): boolean {
    try {
      this.tasks.getTask(taskId)
      return true
    } catch {
      // Deleted meanwhile.
      return false
    }
  }

  // The history of the task branch next to the branch that the main checkout is on. `fresh` as
  // for changes.
  async graph(taskId: string, fresh = false): Promise<GitGraph> {
    const workspace = this.workspaceOf(taskId)
    const repoDir = this.repoDirOf(this.tasks.getTask(taskId).repo)
    if (fresh) {
      this.graphReads.forget(taskId)
    }
    return this.graphReads.get(taskId, async () =>
      readGitGraph(workspace, await currentBranch(repoDir)),
    )
  }

  // Commits every change of the workspace on the task branch.
  async commit(taskId: string, message: string): Promise<{ task: Task; hash: string }> {
    return this.exclusive(taskId, async () => {
      const { hash, subject } = await commitAll(this.workspaceOf(taskId), message)
      const task = this.tasks.recordGit(taskId, 'git.committed', { hash, subject })
      return { task, hash }
    })
  }

  // A commit message for the changes of the workspace, by the task's agent. It only reads, so a
  // push that runs does not stop it. A second ask while one runs gets the same answer.
  async draftCommitMessage(taskId: string): Promise<CommitMessageDraft> {
    const running = this.drafts.get(taskId)
    if (running !== undefined) {
      return running
    }
    const draft = this.draftNow(taskId).finally(() => this.drafts.delete(taskId))
    this.drafts.set(taskId, draft)
    return draft
  }

  private async draftNow(taskId: string): Promise<CommitMessageDraft> {
    const workspace = this.workspaceOf(taskId)
    const task = this.tasks.getTask(taskId)
    const agent = task.agent !== null && isLaunchable(task.agent) ? task.agent : 'claude-code'
    const [diff, changes] = await Promise.all([
      readCommitDiff(workspace, MAX_DIFF_CHARS),
      listChanges(workspace),
    ])
    const draft = await draftCommitMessage(
      { agent, program: this.programOf(agent), cwd: workspace, title: task.title, changes, diff },
      this.runAgentOnce,
    )
    if (draft.source === 'draft') {
      this.log(`No commit message draft by ${agent} for ${taskId}: ${draft.reason}`)
    }
    return draft
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
      // Every change of a workspace runs here, so the next read sees it, even after a failure.
      this.touch(taskId)
    }
  }
}
