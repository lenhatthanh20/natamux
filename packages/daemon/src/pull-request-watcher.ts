import type { Task } from '@natamux/core'
import { GitHubError, type RunGh, readPullRequestStatus, runGh } from './git.js'
import { RepoNotFoundError } from './repo-service.js'
import type { TaskService } from './task-service.js'

export const WATCH_INTERVAL_MS = 60_000

type PullRequestWatcherOptions = {
  tasks: TaskService
  // Where gh runs for the tasks of a repository. A full pull request address works from any folder.
  cwdOf: (repo: string | null) => string
  gh?: RunGh
  intervalMs?: number
  log?: (message: string) => void
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

// Asks GitHub about the pull requests of tasks once a minute, so a task shows its CI and moves to
// done when its pull request is merged.
export class PullRequestWatcher {
  private readonly tasks: TaskService
  private readonly cwdOf: (repo: string | null) => string
  private readonly gh: RunGh
  private readonly intervalMs: number
  private readonly log: (message: string) => void
  private timer: NodeJS.Timeout | null = null
  private running = false
  private lastError: string | null = null

  constructor(options: PullRequestWatcherOptions) {
    this.tasks = options.tasks
    this.cwdOf = options.cwdOf
    this.gh = options.gh ?? runGh
    this.intervalMs = options.intervalMs ?? WATCH_INTERVAL_MS
    this.log = options.log ?? ((message) => console.warn(message))
  }

  start(): void {
    this.timer ??= setInterval(() => void this.checkAll(), this.intervalMs)
    this.timer.unref()
  }

  stop(): void {
    if (this.timer !== null) {
      clearInterval(this.timer)
      this.timer = null
    }
  }

  // Checks every watched task. A failure for one task does not stop the others.
  async checkAll(): Promise<void> {
    if (this.running) {
      return
    }
    this.running = true
    try {
      for (const task of this.tasks.listTasks().filter(isWatched)) {
        await this.check(task.id).catch((error: unknown) => this.report(error))
      }
    } finally {
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

  // The same failure every minute (gh missing, not logged in) is said once.
  private report(error: unknown): void {
    const message = error instanceof GitHubError ? error.message : String(error)
    if (message !== this.lastError) {
      this.lastError = message
      this.log(`Pull request check: ${message}`)
    }
  }
}
