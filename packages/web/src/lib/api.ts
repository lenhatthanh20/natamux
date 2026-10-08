import {
  type ChangeEntry,
  changeEntrySchema,
  type EventType,
  type FileDiff,
  fileDiffSchema,
  type GitGraph,
  gitGraphSchema,
  type Task,
  type TaskEvent,
  taskEventSchema,
  taskSchema,
} from '@helmsman/core'

export type FetchFunction = (input: string, init?: RequestInit) => Promise<Response>

export type SendEventResult = {
  task: Task
  accepted: boolean
}

export type StoredEvent = {
  seq: number
  accepted: boolean
  event: TaskEvent
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

export function isUnauthorized(error: unknown): boolean {
  return error instanceof ApiError && error.status === 401
}

function asRecord(value: unknown): Record<string, unknown> {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) {
    throw new Error('The daemon sent an answer this dashboard does not understand.')
  }
  return Object.fromEntries(Object.entries(value))
}

function parseJson(text: string): unknown {
  try {
    return JSON.parse(text)
  } catch {
    return undefined
  }
}

function describeError(status: number, body: unknown): string {
  if (status === 401) {
    return 'The daemon rejected the token.'
  }
  if (typeof body === 'object' && body !== null && 'message' in body) {
    return String(body.message)
  }
  return `The daemon answered with status ${status}.`
}

export function createApi(
  token: string,
  fetchFunction: FetchFunction = (input, init) => fetch(input, init),
) {
  async function request(method: string, path: string, body?: unknown): Promise<unknown> {
    let response: Response
    try {
      response = await fetchFunction(`/api${path}`, {
        method,
        headers: {
          authorization: `Bearer ${token}`,
          ...(body === undefined ? {} : { 'content-type': 'application/json' }),
        },
        body: body === undefined ? undefined : JSON.stringify(body),
      })
    } catch {
      throw new Error('Cannot reach the daemon. Is it running?')
    }

    const text = await response.text()
    const parsed = text === '' ? undefined : parseJson(text)
    if (!response.ok) {
      throw new ApiError(response.status, describeError(response.status, parsed))
    }
    return parsed
  }

  return {
    async listTasks(): Promise<Task[]> {
      const body = asRecord(await request('GET', '/tasks'))
      if (!Array.isArray(body.tasks)) {
        throw new Error('The daemon sent an answer this dashboard does not understand.')
      }
      return body.tasks.map((task) => taskSchema.parse(task))
    },

    async createTask(title: string): Promise<Task> {
      const body = asRecord(await request('POST', '/tasks', { title }))
      return taskSchema.parse(body.task)
    },

    async sendEvent(taskId: string, type: EventType): Promise<SendEventResult> {
      const body = asRecord(
        await request('POST', `/tasks/${encodeURIComponent(taskId)}/events`, {
          source: 'human',
          type,
        }),
      )
      return { task: taskSchema.parse(body.task), accepted: body.accepted === true }
    },

    async listEvents(taskId: string): Promise<StoredEvent[]> {
      const body = asRecord(await request('GET', `/tasks/${encodeURIComponent(taskId)}/events`))
      if (!Array.isArray(body.events)) {
        throw new Error('The daemon sent an answer this dashboard does not understand.')
      }
      return body.events.map((entry) => {
        const stored = asRecord(entry)
        return {
          seq: Number(stored.seq),
          accepted: stored.accepted === true,
          event: taskEventSchema.parse(stored.event),
        }
      })
    },

    async listChanges(taskId: string): Promise<ChangeEntry[]> {
      const body = asRecord(await request('GET', `/tasks/${encodeURIComponent(taskId)}/changes`))
      if (!Array.isArray(body.changes)) {
        throw new Error('The daemon sent an answer this dashboard does not understand.')
      }
      return body.changes.map((entry) => changeEntrySchema.parse(entry))
    },

    async getFileDiff(taskId: string, path: string): Promise<FileDiff> {
      const body = await request(
        'GET',
        `/tasks/${encodeURIComponent(taskId)}/changes/file?path=${encodeURIComponent(path)}`,
      )
      return fileDiffSchema.parse(body)
    },

    async getGitGraph(taskId: string): Promise<GitGraph> {
      return gitGraphSchema.parse(
        await request('GET', `/tasks/${encodeURIComponent(taskId)}/graph`),
      )
    },

    async createWorkspace(taskId: string): Promise<Task> {
      const body = asRecord(await request('POST', `/tasks/${encodeURIComponent(taskId)}/workspace`))
      return taskSchema.parse(body.task)
    },

    async removeWorkspace(taskId: string): Promise<RemoveWorkspaceResult> {
      const body = asRecord(
        await request('DELETE', `/tasks/${encodeURIComponent(taskId)}/workspace?deleteBranch=true`),
      )
      return { task: taskSchema.parse(body.task), branchDeleted: body.branchDeleted === true }
    },
  }
}

export type Api = ReturnType<typeof createApi>
