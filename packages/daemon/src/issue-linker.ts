import { type IssueLink, stagesOf, type Task } from '@natamux/core'
import type { DaemonMessage, EventBus } from './event-bus.js'
import { GitHubError, type RunGh, runGh } from './git.js'
import { readStageFile } from './stage-files.js'
import type { TaskService } from './task-service.js'

const MAX_BODY_CHARS = 60_000

export class IssueLinkError extends Error {
  constructor(message: string) {
    super(message)
    this.name = 'IssueLinkError'
  }
}

// The issue address `gh issue create` prints.
export function issueUrl(output: string): string | undefined {
  return /https:\/\/\S+\/issues\/\d+/.exec(output)?.[0]
}

// The line a pull request body ends with, so the tracker closes the item on merge.
export function closingLine(issue: IssueLink): string {
  return issue.provider === 'github'
    ? `Closes ${issue.key}`
    : `Tracked in ${issue.key}: ${issue.url}`
}

type IssueLinkerOptions = {
  tasks: TaskService
  bus: EventBus
  dataDir: string
  repoPath: (repo: string | null) => string
  gh?: RunGh
}

// Opens a GitHub issue for a task made in the app, when its project asks for one
// (`tracker.createIssue`), and links the pull request to it.
export class IssueLinker {
  private readonly tasks: TaskService
  private readonly bus: EventBus
  private readonly dataDir: string
  private readonly repoPath: (repo: string | null) => string
  private readonly gh: RunGh
  private readonly pending = new Set<Promise<unknown>>()
  // Issues being opened now, by task: a second request waits for the first, not a second issue.
  private readonly creating = new Map<string, Promise<Task>>()
  private unsubscribe: (() => void) | undefined

  constructor(options: IssueLinkerOptions) {
    this.tasks = options.tasks
    this.bus = options.bus
    this.dataDir = options.dataDir
    this.repoPath = options.repoPath
    this.gh = options.gh ?? runGh
  }

  start(): void {
    this.unsubscribe = this.bus.subscribe((message) => this.onMessage(message))
  }

  stop(): void {
    this.unsubscribe?.()
  }

  // Waits for issues being opened in the background. For tests.
  async settle(): Promise<void> {
    while (this.pending.size > 0) {
      await Promise.allSettled([...this.pending])
    }
  }

  private tracker(task: Pick<Task, 'repo'>) {
    return this.tasks.settingsOf(task.repo).tracker
  }

  private hasSpecStage(task: Task): boolean {
    return this.tasks
      .stepsOf(task)
      .some((step) => stagesOf(step).some((stage) => stage.id === 'spec'))
  }

  private onMessage(message: DaemonMessage): void {
    if (message.kind !== 'task.updated' || !message.accepted) {
      return
    }
    const { task, event } = message
    if (event.type !== 'stage.approved' || event.payload.stage !== 'spec') {
      return
    }
    const tracker = this.tracker(task)
    if (
      tracker.type === 'github' &&
      tracker.createIssue === 'on-spec-approved' &&
      task.issue === null
    ) {
      // A failure leaves the task without an issue; "Create issue" can try again.
      const work = this.create(task.id).catch((error: unknown) => {
        console.error(`Could not open an issue for ${task.id}: ${String(error)}`)
      })
      const tracked = work.finally(() => this.pending.delete(tracked))
      this.pending.add(tracked)
    }
  }

  // Opens the issue now: its title is the task's, its body the spec (else the notes).
  async create(taskId: string): Promise<Task> {
    const running = this.creating.get(taskId)
    if (running !== undefined) {
      return running
    }
    const work = this.createNow(taskId).finally(() => this.creating.delete(taskId))
    this.creating.set(taskId, work)
    return work
  }

  private async createNow(taskId: string): Promise<Task> {
    const task = this.tasks.getTask(taskId)
    if (task.issue !== null) {
      throw new IssueLinkError(`${taskId} is linked to ${task.issue.key} already`)
    }
    if (this.tracker(task).type !== 'github') {
      throw new IssueLinkError(
        'This project has no issue tracker. Set tracker.type in .natamux.json',
      )
    }
    const spec = readStageFile(this.dataDir, task, 'spec.md')
    const body = [
      (spec ?? task.description).trim() || task.title,
      '',
      `Made by Natamux for ${task.id}.`,
    ].join('\n')
    const output = await this.gh(this.repoPath(task.repo), [
      'issue',
      'create',
      '--title',
      task.title,
      '--body',
      body.slice(0, MAX_BODY_CHARS),
    ])
    const url = issueUrl(output)
    if (url === undefined) {
      throw new GitHubError(`no issue address in the answer: ${output.trim()}`)
    }
    const number = /\/issues\/(\d+)/.exec(url)?.[1] ?? ''
    // The task may have been linked while gh ran, for example from its context.
    const current = this.tasks.getTask(taskId).issue
    if (current !== null) {
      this.tasks.recordEvent(task.id, 'issue.created', { url })
      throw new IssueLinkError(
        `${taskId} was linked to ${current.key} while ${url} was opened. Close one of them`,
      )
    }
    const linked = this.tasks.linkIssue(
      task.id,
      { provider: 'github', key: `#${number}`, url },
      'system',
    )
    this.tasks.recordEvent(task.id, 'issue.created', { url })
    return linked
  }

  // The body of the task's pull request. Opens the issue first when the project wants one by
  // now: `on-pr`, or `on-spec-approved` in a pipeline without a spec stage.
  async pullRequestBody(task: Task): Promise<string> {
    const tracker = this.tracker(task)
    const wantsIssue =
      tracker.type === 'github' &&
      (tracker.createIssue === 'on-pr' ||
        (tracker.createIssue === 'on-spec-approved' && !this.hasSpecStage(task)))
    const current = wantsIssue && task.issue === null ? await this.create(task.id) : task
    const body = current.description.trim() || `Task ${current.id} in Natamux.`
    return tracker.linkInPr && current.issue !== null
      ? `${body}\n\n${closingLine(current.issue)}`
      : body
  }
}
