import type { Task } from '@natamux/core'
import type { EventBus } from './event-bus.js'
import {
  GitHubError,
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
}

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
// done when its pull request is merged. One gh call per GitHub repository.
export class PullRequestWatcher {
  private readonly tasks: TaskService
  private readonly cwdOf: (repo: string | null) => string
  private readonly gh: RunGh
  private readonly intervalMs: number
  private readonly log: (message: string) => void
  private readonly bus: EventBus | undefined
  private timer: NodeJS.Timeout | null = null
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
    if (this.hasClients()) {
      void this.checkAll()
    }
  }

  stop(): void {
    if (this.timer !== null) {
      clearInterval(this.timer)
      this.timer = null
    }
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
    const task = this.tasks.getTask(taskId)
    if (task.pr === null) {
      return task
    }
    const status = await readPullRequestStatus(this.cwdOf(task.repo), task.pr, this.gh)
    return this.tasks.recordPullRequestStatus(taskId, status)
  }

  private hasClients(): boolean {
    return this.bus === undefined || this.bus.size() > 0
  }

  // Like the auto fetch of VS Code: slower when nothing is about to change, and after failures.
  private due(): boolean {
    if (!this.hasClients()) {
      return false
    }
    const settled = this.tasks.listTasks().filter(isWatched).every(isSettled)
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
