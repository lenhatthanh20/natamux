import type { Task } from '@natamux/core'
import { GitHubError, type RunGh, readPullRequestStatus, runGh } from './git.js'
import type { TaskService } from './task-service.js'

export const WATCH_INTERVAL_MS = 60_000

type PullRequestWatcherOptions = {
  tasks: TaskService
  // Where gh runs. A full pull request address works from any folder of a repository.
  cwd: string
  gh?: RunGh
  intervalMs?: number
  log?: (message: string) => void
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
  private readonly cwd: string
  private readonly gh: RunGh
  private readonly intervalMs: number
  private readonly log: (message: string) => void
  private timer: NodeJS.Timeout | null = null
  private running = false
  private lastError: string | null = null

  constructor(options: PullRequestWatcherOptions) {
    this.tasks = options.tasks
    this.cwd = options.cwd
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
    const status = await readPullRequestStatus(this.cwd, task.pr, this.gh)
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
