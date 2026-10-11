import {
  type AttachmentUpload,
  type Repo,
  repoSchema,
  type Task,
  type TaskEvent,
  type TaskState,
  taskEventSchema,
  taskSchema,
} from '@natamux/core'
import type { TaskPriority } from './command.js'

export type FetchFunction = (input: string, init?: RequestInit) => Promise<Response>

export type CreateTaskRequest = {
  title: string
  description?: string
  priority?: TaskPriority
  labels?: string[]
  agent?: string
  dependsOn?: string[]
  context?: { source: string; ref: string }
  // Null is the project with the id "default".
  repo?: string | null
  attachments?: AttachmentUpload[]
}

export type StageActionRequest = {
  action: 'approve' | 'reopen' | 'skip' | 'pass' | 'fail' | 'report' | 'start'
  stage?: string
  terminal?: string
  to?: string
  reason?: string
}

export type SendEventRequest = {
  source: string
  type: string
  payload?: Record<string, unknown>
}

export type StoredEvent = {
  seq: number
  accepted: boolean
  event: TaskEvent
}

export type SendEventResult = {
  task: Task
  accepted: boolean
}

export type RemoveWorkspaceResult = {
  task: Task
  branchDeleted: boolean
}

export class ApiError extends Error {
  readonly status: number

  constructor(status: number, message: string) {
    super(message)
    this.name = 'ApiError'
    this.status = status
  }
}

type ClientOptions = {
  baseUrl: string
  token: string
  fetch: FetchFunction
}

function asRecord(value: unknown, what: string): Record<string, unknown> {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) {
    throw new Error(`Unexpected answer from the daemon: ${what}`)
  }
  return Object.fromEntries(Object.entries(value))
}

function asArray(value: unknown, what: string): unknown[] {
  if (!Array.isArray(value)) {
    throw new Error(`Unexpected answer from the daemon: ${what}`)
  }
  return value
}

function parseJson(text: string): unknown {
  try {
    return JSON.parse(text)
  } catch {
    return undefined
  }
}

function describeIssue(issue: unknown): string {
  const { path, message } = asRecord(issue, 'issue')
  const where = Array.isArray(path) ? path.join('.') : ''
  return where === '' ? String(message) : `${where}: ${String(message)}`
}

function describeError(status: number, body: unknown): string {
  if (status === 401) {
    return 'The daemon rejected the token. Check NATAMUX_TOKEN.'
  }
  if (typeof body === 'object' && body !== null && !Array.isArray(body)) {
    const fields = Object.fromEntries(Object.entries(body))
    if (fields.error === 'invalid_request' && Array.isArray(fields.issues)) {
      return `Invalid request. ${fields.issues.map(describeIssue).join('; ')}`
    }
    if (typeof fields.message === 'string') {
      return fields.message
    }
  }
  return `The daemon answered with status ${status}.`
}

export class DaemonClient {
  private readonly baseUrl: string
  private readonly token: string
  private readonly fetchFunction: FetchFunction

  constructor(options: ClientOptions) {
    this.baseUrl = options.baseUrl.replace(/\/+$/, '')
    this.token = options.token
    this.fetchFunction = options.fetch
  }

  async listTasks(state?: TaskState): Promise<Task[]> {
    const query = state === undefined ? '' : `?state=${encodeURIComponent(state)}`
    const body = asRecord(await this.request('GET', `/tasks${query}`), 'task list')
    return asArray(body.tasks, 'task list').map((task) => taskSchema.parse(task))
  }

  async createTask(request: CreateTaskRequest): Promise<Task> {
    const body = asRecord(await this.request('POST', '/tasks', request), 'created task')
    return taskSchema.parse(body.task)
  }

  async listRepos(): Promise<Repo[]> {
    const body = asRecord(await this.request('GET', '/repos'), 'project list')
    return asArray(body.repos, 'project list').map((repo) => repoSchema.parse(repo))
  }

  async getTask(id: string): Promise<Task> {
    const body = asRecord(await this.request('GET', this.taskPath(id)), 'task')
    return taskSchema.parse(body.task)
  }

  async sendEvent(id: string, request: SendEventRequest): Promise<SendEventResult> {
    const body = asRecord(
      await this.request('POST', `${this.taskPath(id)}/events`, request),
      'event result',
    )
    return { task: taskSchema.parse(body.task), accepted: body.accepted === true }
  }

  async listEvents(id: string): Promise<StoredEvent[]> {
    const body = asRecord(await this.request('GET', `${this.taskPath(id)}/events`), 'event log')
    return asArray(body.events, 'event log').map((entry) => {
      const stored = asRecord(entry, 'event log entry')
      return {
        seq: Number(stored.seq),
        accepted: stored.accepted === true,
        event: taskEventSchema.parse(stored.event),
      }
    })
  }

  async updateTask(id: string, changes: Record<string, unknown>): Promise<Task> {
    const body = asRecord(await this.request('PATCH', this.taskPath(id), changes), 'edited task')
    return taskSchema.parse(body.task)
  }

  // A task with no workspace gets one first, from `base` when it is given.
  async startAgent(id: string, agent: string, prompt?: string, base?: string): Promise<Task> {
    const body = asRecord(
      await this.request('POST', `${this.taskPath(id)}/agent`, { agent, prompt, base }),
      'started agent',
    )
    return taskSchema.parse(body.task)
  }

  async stageAction(id: string, request: StageActionRequest): Promise<Task> {
    const body = asRecord(await this.request('POST', `${this.taskPath(id)}/stage`, request), 'task')
    return taskSchema.parse(body.task)
  }

  async linkIssue(id: string, link: { provider: string; key: string; url: string }): Promise<Task> {
    const body = asRecord(await this.request('PUT', `${this.taskPath(id)}/issue`, link), 'task')
    return taskSchema.parse(body.task)
  }

  async createWorkspace(id: string, base?: string): Promise<Task> {
    const body = asRecord(
      await this.request(
        'POST',
        `${this.taskPath(id)}/workspace`,
        base === undefined ? undefined : { base },
      ),
      'created workspace',
    )
    return taskSchema.parse(body.task)
  }

  async removeWorkspace(id: string, deleteBranch: boolean): Promise<RemoveWorkspaceResult> {
    const body = asRecord(
      await this.request('DELETE', `${this.taskPath(id)}/workspace?deleteBranch=${deleteBranch}`),
      'removed workspace',
    )
    return { task: taskSchema.parse(body.task), branchDeleted: body.branchDeleted === true }
  }

  private taskPath(id: string): string {
    return `/tasks/${encodeURIComponent(id)}`
  }

  private async request(method: string, path: string, body?: unknown): Promise<unknown> {
    let response: Response
    try {
      response = await this.fetchFunction(`${this.baseUrl}${path}`, {
        method,
        // The daemon refuses a request that says it is JSON but has no body.
        headers: {
          authorization: `Bearer ${this.token}`,
          ...(body === undefined ? {} : { 'content-type': 'application/json' }),
        },
        body: body === undefined ? undefined : JSON.stringify(body),
      })
    } catch {
      throw new Error(`Cannot reach the daemon at ${this.baseUrl}. Is it running?`)
    }

    const text = await response.text()
    const parsed = text === '' ? undefined : parseJson(text)
    if (!response.ok) {
      throw new ApiError(response.status, describeError(response.status, parsed))
    }
    return parsed
  }
}
