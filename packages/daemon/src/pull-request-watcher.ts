import type { EventType, Task } from '@natamux/core'
import type { DaemonMessage, EventBus } from './event-bus.js'
import {
  findPullRequest,
  findPullRequests,
  GitHubError,
  hasRemoteBranch,
  type PullRequestRef,
  parsePullRequestRef,
  type RunGh,
  readPullRequestStatus,
  readPullRequestStatuses,
  runGh,
} from './git.js'
import { RepoNotFoundError } from './repo-service.js'
import type { TaskService } from './task-service.js'

export const WATCH_INTERVAL_MS = 60_000
// How often GitHub is asked once no watched pull request waits for its checks. Failures in a row
// slow the checks down to this too.
export const SETTLED_INTERVAL_MS = 5 * 60_000

type PullRequestWatcherOptions = {
  tasks: TaskService
  // Where gh runs for the tasks of a repository. A full pull request address works from any folder.
  cwdOf: (repo: string | null) => string
  gh?: RunGh
  intervalMs?: number
  log?: (message: string) => void
  // Only dashboards show the status, so the timed checks rest while none listens. Without a bus
  // they always run.
  bus?: EventBus
  hasRemoteBranch?: (worktreePath: string, branch: string) => Promise<boolean>
}

// An agent usually opens its pull request just before its turn ends.
const AGENT_STOP_EVENTS: readonly EventType[] = [
  'agent.finished',
  'agent.waiting',
  'agent.stopped',
  'stage.reported',
]

// The folder of a task's repository for gh. A removed repository has none; the fallback (the data
// folder) does, and a full pull request address works from there too.
export function repoFolderOr(
  repos: { pathOf: (repo: string | null) => string },
  fallback: string,
): (repo: string | null) => string {
  return (repo) => {
    try {
      return repos.pathOf(repo)
    } catch (error) {
      if (error instanceof RepoNotFoundError) {
        return fallback
      }
      throw error
    }
  }
}

// A task is watched while it has an open pull request and is not cancelled.
export function isWatched(task: Task): boolean {
  if (task.pr === null || task.state === 'cancelled') {
    return false
  }
  return task.prStatus === null || task.prStatus.state === 'open' || task.prStatus.state === 'draft'
}

export type Candidate = Task & { branch: string; workspace: string }

// A task whose pull request may have been opened outside Natamux, by its agent or on GitHub.
export function isCandidate(task: Task): task is Candidate {
  return (
    task.pr === null &&
    task.branch !== null &&
    task.workspace !== null &&
    task.state !== 'cancelled' &&
    task.state !== 'done'
  )
}

// Settled: the status is known and no check runs, so it changes only by people.
function isSettled(task: Task): boolean {
  return task.prStatus !== null && task.prStatus.checks !== 'pending'
}

// The watched tasks of one GitHub repository. `folder`: the Natamux repository of the first one.
type RepoBatch = {
  folder: string | null
  repo: PullRequestRef
  tasks: { id: string; number: number }[]
}

// Asks GitHub about the pull requests of tasks once a minute, so a task shows its CI and moves to
// done when its pull request is merged. One gh call per GitHub repository. It also links a pull
// request that the agent or a person opened from the task branch outside Natamux, with one gh call
// per project.
export class PullRequestWatcher {
  private readonly tasks: TaskService
  private readonly cwdOf: (repo: string | null) => string
  private readonly gh: RunGh
  private readonly intervalMs: number
  private readonly log: (message: string) => void
  private readonly bus: EventBus | undefined
  private readonly hasRemoteBranch: (worktreePath: string, branch: string) => Promise<boolean>
  private timer: NodeJS.Timeout | null = null
  private unsubscribe: (() => void) | null = null
  // Tasks whose pull request is being looked for now.
  private readonly finding = new Set<string>()
  // Tasks without a pull request whose branch origin had at the last look. Kept here because due()
  // cannot ask git.
  private readonly onOrigin = new Set<string>()
  private running = false
  private lastError: string | null = null
  private lastCheckAt = Number.NEGATIVE_INFINITY
  // Checks in a row that had a failure.
  private failures = 0

  constructor(options: PullRequestWatcherOptions) {
    this.tasks = options.tasks
    this.cwdOf = options.cwdOf
    this.gh = options.gh ?? runGh
    this.intervalMs = options.intervalMs ?? WATCH_INTERVAL_MS
    this.log = options.log ?? ((message) => console.warn(message))
    this.bus = options.bus
    this.hasRemoteBranch = options.hasRemoteBranch ?? hasRemoteBranch
  }

  // The timer looks every intervalMs; whether it asks GitHub depends on the dashboards, the
  // checks and the failures (see due).
  start(): void {
    if (this.timer !== null) {
      return
    }
    this.timer = setInterval(() => {
      if (this.due()) {
        void this.checkAll()
      }
    }, this.intervalMs)
    this.timer.unref()
    this.unsubscribe = this.bus?.subscribe((message) => this.onMessage(message)) ?? null
    if (this.hasClients()) {
      void this.checkAll()
    }
  }

  stop(): void {
    if (this.timer !== null) {
      clearInterval(this.timer)
      this.timer = null
    }
    this.unsubscribe?.()
    this.unsubscribe = null
  }

  // A dashboard came while none listened: it gets fresh statuses, unless a check ran just now.
  wake(): void {
    if (this.timer !== null && Date.now() - this.lastCheckAt >= this.intervalMs) {
      void this.checkAll()
    }
  }

  // Checks every watched task. A failure for one repository does not stop the others.
  async checkAll(): Promise<void> {
    if (this.running) {
      return
    }
    this.running = true
    this.lastCheckAt = Date.now()
    let failed = false
    const report = (error: unknown) => {
      failed = true
      this.report(error)
    }
    try {
      // The batch below reads the status of a pull request found here.
      await this.findPullRequests(report)
      const batches = new Map<string, RepoBatch>()
      for (const task of this.tasks.listTasks().filter(isWatched)) {
        const ref = parsePullRequestRef(task.pr ?? '')
        if (ref === undefined) {
          // Not an address gh gives; it is asked about alone.
          await this.check(task.id).catch(report)
          continue
        }
        const key = `${ref.host}/${ref.owner}/${ref.name}`
        const batch: RepoBatch = batches.get(key) ?? { folder: task.repo, repo: ref, tasks: [] }
        batch.tasks.push({ id: task.id, number: ref.number })
        batches.set(key, batch)
      }
      for (const { folder, repo, tasks } of batches.values()) {
        try {
          const numbers = tasks.map((task) => task.number)
          const statuses = await readPullRequestStatuses(this.cwdOf(folder), repo, numbers, this.gh)
          for (const task of tasks) {
            const status = statuses.get(task.number)
            if (status !== undefined) {
              this.tasks.recordPullRequestStatus(task.id, status)
            }
          }
        } catch (error) {
          report(error)
        }
      }
    } finally {
      this.failures = failed ? this.failures + 1 : 0
      this.running = false
    }
  }

  async check(taskId: string): Promise<Task> {
    if (this.tasks.getTask(taskId).pr === null) {
      await this.findPullRequest(taskId)
    }
    const task = this.tasks.getTask(taskId)
    if (task.pr === null) {
      return task
    }
    const status = await readPullRequestStatus(this.cwdOf(task.repo), task.pr, this.gh)
    return this.tasks.recordPullRequestStatus(taskId, status)
  }

  // Links the pull request from the task branch, when there is one. Asks GitHub only once origin
  // has the branch.
  private async findPullRequest(taskId: string): Promise<void> {
    const task = this.tasks.getTask(taskId)
    if (!isCandidate(task) || this.finding.has(taskId)) {
      return
    }
    this.finding.add(taskId)
    try {
      if (!(await this.hasRemoteBranch(task.workspace, task.branch))) {
        this.onOrigin.delete(taskId)
        return
      }
      this.onOrigin.add(taskId)
      const url = await findPullRequest(task.workspace, task.branch, this.gh)
      if (url !== undefined && this.tasks.getTask(taskId).pr === null) {
        this.tasks.recordGit(taskId, 'pr.opened', { url, detected: 'true' })
        this.onOrigin.delete(taskId)
      }
    } finally {
      this.finding.delete(taskId)
    }
  }

  // Links the pull requests of all candidates, with one gh call per project: a call per task
  // would use up the GitHub limit that gh shares with the user. Runs gh in the workspace of the
  // first task of the project.
  private async findPullRequests(report: (error: unknown) => void): Promise<void> {
    const byRepo = new Map<string | null, { cwd: string; tasks: Candidate[] }>()
    for (const task of this.tasks.listTasks().filter(isCandidate)) {
      if (this.finding.has(task.id)) {
        continue
      }
      try {
        if (!(await this.hasRemoteBranch(task.workspace, task.branch))) {
          this.onOrigin.delete(task.id)
          continue
        }
      } catch (error) {
        report(error)
        continue
      }
      this.onOrigin.add(task.id)
      const batch = byRepo.get(task.repo) ?? { cwd: task.workspace, tasks: [] }
      batch.tasks.push(task)
      byRepo.set(task.repo, batch)
    }
    for (const { cwd, tasks } of byRepo.values()) {
      try {
        const urls = await findPullRequests(cwd, this.gh)
        for (const task of tasks) {
          const url = urls.get(task.branch)
          // A check may have linked it while gh ran.
          if (url !== undefined && this.tasks.getTask(task.id).pr === null) {
            this.tasks.recordGit(task.id, 'pr.opened', { url, detected: 'true' })
            this.onOrigin.delete(task.id)
          }
        }
      } catch (error) {
        report(error)
      }
    }
  }

  private onMessage(message: DaemonMessage): void {
    if (
      message.kind === 'task.updated' &&
      AGENT_STOP_EVENTS.includes(message.event.type) &&
      isCandidate(message.task)
    ) {
      void this.check(message.task.id).catch((error: unknown) => this.report(error))
    }
  }

  private hasClients(): boolean {
    return this.bus === undefined || this.bus.size() > 0
  }

  // Like the auto fetch of VS Code: slower when nothing is about to change, and after failures.
  private due(): boolean {
    if (!this.hasClients()) {
      return false
    }
    const tasks = this.tasks.listTasks()
    // A pushed branch without a pull request may get one any minute.
    const pushed = tasks.some((task) => isCandidate(task) && this.onOrigin.has(task.id))
    const settled = !pushed && tasks.filter(isWatched).every(isSettled)
    const base = settled ? SETTLED_INTERVAL_MS : this.intervalMs
    const wait = Math.min(base * 2 ** this.failures, SETTLED_INTERVAL_MS)
    // Half a tick of slack: by the wall clock, a tick may come a moment before the full wait.
    return Date.now() - this.lastCheckAt >= wait - this.intervalMs / 2
  }

  // The same failure every minute (gh missing, not logged in) is said once.
  private report(error: unknown): void {
    const message = error instanceof GitHubError ? error.message : String(error)
    if (message !== this.lastError) {
      this.lastError = message
      this.log(`Pull request check: ${message}`)
    }
  }
}
