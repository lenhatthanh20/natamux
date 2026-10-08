import {
  type ChangeEntry,
  changeEntrySchema,
  type DiffStat,
  diffStatSchema,
  type EventType,
  type FileDiff,
  fileDiffSchema,
  type GitGraph,
  type GitHubIssue,
  gitGraphSchema,
  githubIssueSchema,
  type Task,
  type TaskEvent,
  type TaskTemplate,
  taskEventSchema,
  taskSchema,
  taskTemplateSchema,
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

export type TaskChanges = Partial<
  Pick<Task, 'title' | 'description' | 'priority' | 'labels' | 'dependsOn'>
>

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

function stringList(value: unknown): string[] {
  if (!Array.isArray(value) || !value.every((item) => typeof item === 'string')) {
    throw new Error('The daemon sent an answer this dashboard does not understand.')
  }
  return value
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

    async updateTask(taskId: string, changes: TaskChanges): Promise<Task> {
      const body = asRecord(await request('PATCH', `/tasks/${encodeURIComponent(taskId)}`, changes))
      return taskSchema.parse(body.task)
    },

    // Copies of a task for several agents to try the same work; with `start`, each gets a
    // workspace and Claude Code.
    async fanOut(taskId: string, count: number, start: boolean): Promise<Task[]> {
      const body = asRecord(
        await request('POST', `/tasks/${encodeURIComponent(taskId)}/fan-out`, { count, start }),
      )
      if (!Array.isArray(body.tasks)) {
        throw new Error('The daemon sent an answer this dashboard does not understand.')
      }
      return body.tasks.map((task) => taskSchema.parse(task))
    },

    // Starts an agent in the terminal of the task's workspace.
    async startAgent(taskId: string, agent: string, prompt?: string): Promise<Task> {
      const body = asRecord(
        await request('POST', `/tasks/${encodeURIComponent(taskId)}/agent`, { agent, prompt }),
      )
      return taskSchema.parse(body.task)
    },

    // Pastes text into the agent's terminal as its next prompt.
    async sendToAgent(taskId: string, text: string): Promise<Task> {
      const body = asRecord(
        await request('POST', `/tasks/${encodeURIComponent(taskId)}/agent/message`, { text }),
      )
      return taskSchema.parse(body.task)
    },

    // Commits every change of the task's workspace.
    async commitChanges(taskId: string, message: string): Promise<Task> {
      const body = asRecord(
        await request('POST', `/tasks/${encodeURIComponent(taskId)}/commit`, { message }),
      )
      return taskSchema.parse(body.task)
    },

    async pushBranch(taskId: string): Promise<Task> {
      const body = asRecord(await request('POST', `/tasks/${encodeURIComponent(taskId)}/push`))
      return taskSchema.parse(body.task)
    },

    // Pushes the branch, then opens a pull request. The task that comes back has its address.
    async openPullRequest(taskId: string): Promise<Task> {
      const body = asRecord(
        await request('POST', `/tasks/${encodeURIComponent(taskId)}/pull-request`),
      )
      return taskSchema.parse(body.task)
    },

    // What the agent of each task does now, by task id, such as "Edit src/a.ts".
    async listActivity(): Promise<Record<string, string>> {
      const activity = asRecord(asRecord(await request('GET', '/activity')).activity)
      return Object.fromEntries(
        Object.entries(activity).flatMap(([taskId, entry]) => {
          const text = asRecord(entry).text
          return typeof text === 'string' ? [[taskId, text]] : []
        }),
      )
    },

    // The size of the work of every task with a workspace, by task id.
    async listDiffStats(): Promise<Record<string, DiffStat>> {
      const stats = asRecord(asRecord(await request('GET', '/diffstats')).stats)
      return Object.fromEntries(
        Object.entries(stats).map(([taskId, stat]) => [taskId, diffStatSchema.parse(stat)]),
      )
    },

    async listTemplates(): Promise<TaskTemplate[]> {
      const body = asRecord(await request('GET', '/templates'))
      if (!Array.isArray(body.templates)) {
        throw new Error('The daemon sent an answer this dashboard does not understand.')
      }
      return body.templates.map((template) => taskTemplateSchema.parse(template))
    },

    async saveTemplate(template: Omit<TaskTemplate, 'id'>): Promise<TaskTemplate> {
      const body = asRecord(await request('POST', '/templates', template))
      return taskTemplateSchema.parse(body.template)
    },

    async createTaskFromTemplate(templateId: string, title: string): Promise<Task> {
      const body = asRecord(
        await request('POST', `/templates/${encodeURIComponent(templateId)}/tasks`, { title }),
      )
      return taskSchema.parse(body.task)
    },

    async listIssues(): Promise<GitHubIssue[]> {
      const body = asRecord(await request('GET', '/github/issues'))
      if (!Array.isArray(body.issues)) {
        throw new Error('The daemon sent an answer this dashboard does not understand.')
      }
      return body.issues.map((issue) => githubIssueSchema.parse(issue))
    },

    // Makes a draft task from each issue. Issues imported before are skipped.
    async importIssues(numbers: readonly number[]): Promise<Task[]> {
      const body = asRecord(await request('POST', '/github/issues/import', { numbers }))
      if (!Array.isArray(body.tasks)) {
        throw new Error('The daemon sent an answer this dashboard does not understand.')
      }
      return body.tasks.map((task) => taskSchema.parse(task))
    },

    // Asks the daemon to read the pull request of the task from GitHub now.
    async refreshPullRequest(taskId: string): Promise<Task> {
      const body = asRecord(
        await request('POST', `/tasks/${encodeURIComponent(taskId)}/pull-request/refresh`),
      )
      return taskSchema.parse(body.task)
    },

    // The names of the open terminals of a task, such as ["1", "2"].
    async listTerminals(taskId: string): Promise<string[]> {
      const body = asRecord(await request('GET', `/tasks/${encodeURIComponent(taskId)}/terminals`))
      return stringList(body.terminals)
    },

    async closeTerminal(taskId: string, terminal: string): Promise<string[]> {
      const body = asRecord(
        await request(
          'DELETE',
          `/tasks/${encodeURIComponent(taskId)}/terminals/${encodeURIComponent(terminal)}`,
        ),
      )
      return stringList(body.terminals)
    },

    // A one-time ticket that opens the terminal socket of a task, see terminalSocketUrl.
    async terminalTicket(taskId: string): Promise<string> {
      const body = asRecord(
        await request('POST', `/tasks/${encodeURIComponent(taskId)}/terminal/ticket`),
      )
      if (typeof body.ticket !== 'string') {
        throw new Error('The daemon sent an answer this dashboard does not understand.')
      }
      return body.ticket
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
