import {
  type Attachment,
  type AttachmentUpload,
  attachmentSchema,
  type ChangeEntry,
  type ContextSourceType,
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
  type ImageSide,
  type ProjectSettings,
  projectSettingsSchema,
  type Repo,
  repoSchema,
  type SkillInfo,
  type SkillList,
  type SkillOrigin,
  type Task,
  type TaskEvent,
  type TaskTemplate,
  type TerminalLane,
  taskEventSchema,
  taskSchema,
  taskTemplateSchema,
  terminalLaneSchema,
} from '@natamux/core'

export type NewWorkspaceInput = {
  title: string
  repo?: string
  base?: string
  agent?: string
  prompt?: string
  context?: { source: ContextSourceType; name?: string; ref: string }
  attachments?: AttachmentUpload[]
}

export type StageAction = 'approve' | 'reopen' | 'skip' | 'pass' | 'fail' | 'report' | 'start'

export type StageActionInput = { stage?: string; to?: string; reason?: string }

export type StageFile = { name: string; size: number; modifiedAt: string }

export type LoadedSettings = {
  settings: ProjectSettings
  error: string | null
  found: boolean
  path: string
  // The text of the file, so an editor keeps what it does not know. Null without a file.
  text: string | null
  // Sent back with a save, so a file that changed since is not replaced unseen. Null when the
  // daemon gives none.
  version: string | null
}

export type SavedSettings = {
  settings: ProjectSettings
  version: string | null
}

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

// What the agents do now, such as "Edit src/a.ts": the latest line of each task, and the line of
// each terminal of each task.
export type AgentActivity = {
  byTask: Record<string, string>
  byTerminal: Record<string, Record<string, string>>
}

// The open shells of a task: the lane of each one where an agent runs, and the program in front
// of each one, or null where the platform does not tell.
export type TaskShells = {
  terminals: string[]
  lanes: Record<string, TerminalLane>
  programs: Record<string, string | null>
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

// The daemon refused a save because the file changed since it was read.
export function isConflict(error: unknown): boolean {
  return error instanceof ApiError && error.status === 409
}

const SKILL_ORIGINS: readonly SkillOrigin[] = ['project', 'plugin', 'user']

function skillInfo(value: unknown): SkillInfo {
  const skill = asRecord(value)
  const origin = SKILL_ORIGINS.find((candidate) => candidate === skill.origin)
  if (typeof skill.name !== 'string' || origin === undefined) {
    throw new Error('The daemon sent an answer this dashboard does not understand.')
  }
  return {
    name: skill.name,
    description: typeof skill.description === 'string' ? skill.description : '',
    origin,
    agents: Array.isArray(skill.agents) ? stringList(skill.agents) : [],
    path: typeof skill.path === 'string' ? skill.path : '',
  }
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
  async function send(
    method: string,
    path: string,
    body?: unknown,
    headers: Record<string, string> = {},
  ): Promise<Response> {
    try {
      return await fetchFunction(`/api${path}`, {
        method,
        headers: {
          authorization: `Bearer ${token}`,
          ...(body === undefined ? {} : { 'content-type': 'application/json' }),
          ...headers,
        },
        body: body === undefined ? undefined : JSON.stringify(body),
      })
    } catch {
      throw new Error('Cannot reach the daemon. Is it running?')
    }
  }

  async function failure(response: Response): Promise<ApiError> {
    const text = await response.text()
    const parsed = text === '' ? undefined : parseJson(text)
    return new ApiError(response.status, describeError(response.status, parsed))
  }

  async function request(
    method: string,
    path: string,
    body?: unknown,
    headers?: Record<string, string>,
  ): Promise<unknown> {
    const response = await send(method, path, body, headers)
    if (!response.ok) {
      throw await failure(response)
    }
    const text = await response.text()
    return text === '' ? undefined : parseJson(text)
  }

  // For an answer that is not JSON, such as an image.
  async function requestBlob(path: string): Promise<Blob> {
    const response = await send('GET', path)
    if (!response.ok) {
      throw await failure(response)
    }
    return response.blob()
  }

  return {
    async listTasks(): Promise<Task[]> {
      const body = asRecord(await request('GET', '/tasks'))
      if (!Array.isArray(body.tasks)) {
        throw new Error('The daemon sent an answer this dashboard does not understand.')
      }
      return body.tasks.map((task) => taskSchema.parse(task))
    },

    async createTask(title: string, parentId?: string, repo?: string): Promise<Task> {
      const body = asRecord(await request('POST', '/tasks', { title, parentId, repo }))
      return taskSchema.parse(body.task)
    },

    // A task, its worktree and, when one is named, its agent, in one step.
    async newWorkspace(input: NewWorkspaceInput): Promise<Task> {
      const body = asRecord(await request('POST', '/workspaces', input))
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

    // One side of a changed image: the last commit (original) or the file now (modified).
    async getFileImage(taskId: string, path: string, side: ImageSide): Promise<Blob> {
      const query = `path=${encodeURIComponent(path)}&side=${side}`
      return requestBlob(`/tasks/${encodeURIComponent(taskId)}/changes/image?${query}`)
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

    // Starts an agent in the terminal of the task's workspace. A task with no workspace gets one
    // first, from `base` when it is given.
    async startAgent(taskId: string, agent: string, prompt?: string, base?: string): Promise<Task> {
      const body = asRecord(
        await request('POST', `/tasks/${encodeURIComponent(taskId)}/agent`, {
          agent,
          prompt,
          base,
        }),
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

    async listActivity(): Promise<AgentActivity> {
      const activity = asRecord(asRecord(await request('GET', '/activity')).activity)
      const result: AgentActivity = { byTask: {}, byTerminal: {} }
      for (const [taskId, terminals] of Object.entries(activity)) {
        const lines: Record<string, string> = {}
        let latest = ''
        for (const [terminal, entry] of Object.entries(asRecord(terminals))) {
          const { text, at } = asRecord(entry)
          if (typeof text !== 'string') {
            continue
          }
          lines[terminal] = text
          // ISO times sort as text.
          const when = typeof at === 'string' ? at : ''
          if (result.byTask[taskId] === undefined || when > latest) {
            result.byTask[taskId] = text
            latest = when
          }
        }
        result.byTerminal[taskId] = lines
      }
      return result
    },

    // The size of the work of every task with a workspace, by task id.
    async listDiffStats(): Promise<Record<string, DiffStat>> {
      const stats = asRecord(asRecord(await request('GET', '/diffstats')).stats)
      return Object.fromEntries(
        Object.entries(stats).map(([taskId, stat]) => [taskId, diffStatSchema.parse(stat)]),
      )
    },

    async listRepos(): Promise<Repo[]> {
      const body = asRecord(await request('GET', '/repos'))
      if (!Array.isArray(body.repos)) {
        throw new Error('The daemon sent an answer this dashboard does not understand.')
      }
      return body.repos.map((repo) => repoSchema.parse(repo))
    },

    async addRepo(path: string, name?: string): Promise<Repo> {
      const body = asRecord(await request('POST', '/repos', { path, name }))
      return repoSchema.parse(body.repo)
    },

    async removeRepo(id: string): Promise<void> {
      await request('DELETE', `/repos/${encodeURIComponent(id)}`)
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

    async createTaskFromTemplate(templateId: string, title: string, repo?: string): Promise<Task> {
      const path = `/templates/${encodeURIComponent(templateId)}/tasks`
      const body = asRecord(await request('POST', path, { title, repo }))
      return taskSchema.parse(body.task)
    },

    async listIssues(repo?: string): Promise<GitHubIssue[]> {
      const query = repo === undefined ? '' : `?repo=${encodeURIComponent(repo)}`
      const body = asRecord(await request('GET', `/github/issues${query}`))
      if (!Array.isArray(body.issues)) {
        throw new Error('The daemon sent an answer this dashboard does not understand.')
      }
      return body.issues.map((issue) => githubIssueSchema.parse(issue))
    },

    // Makes a draft task from each issue. Issues imported before are skipped.
    async importIssues(numbers: readonly number[], repo?: string): Promise<Task[]> {
      const body = asRecord(await request('POST', '/github/issues/import', { numbers, repo }))
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

    // The terminals where an agent runs now, by name, with the stage each one works on.
    async listAgentTerminals(taskId: string): Promise<Record<string, TerminalLane>> {
      const body = asRecord(await request('GET', `/tasks/${encodeURIComponent(taskId)}/terminals`))
      const lanes = body.lanes === undefined ? {} : asRecord(body.lanes)
      return Object.fromEntries(
        stringList(body.agents).map((name) => [
          name,
          terminalLaneSchema.parse(lanes[name] ?? { stage: null, state: null }),
        ]),
      )
    },

    async listShells(taskId: string): Promise<TaskShells> {
      const body = asRecord(await request('GET', `/tasks/${encodeURIComponent(taskId)}/terminals`))
      const lanes = body.lanes === undefined ? {} : asRecord(body.lanes)
      const programs = body.programs === undefined ? {} : asRecord(body.programs)
      const terminals = stringList(body.terminals)
      return {
        terminals,
        lanes: Object.fromEntries(
          stringList(body.agents).map((name) => [
            name,
            terminalLaneSchema.parse(lanes[name] ?? { stage: null, state: null }),
          ]),
        ),
        programs: Object.fromEntries(
          terminals.map((name) => {
            const program = programs[name]
            return [name, typeof program === 'string' ? program : null]
          }),
        ),
      }
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

    // From `base`, a local branch, or from the checkout's HEAD when it is left out.
    async createWorkspace(taskId: string, base?: string): Promise<Task> {
      const body = asRecord(
        await request(
          'POST',
          `/tasks/${encodeURIComponent(taskId)}/workspace`,
          base === undefined ? undefined : { base },
        ),
      )
      return taskSchema.parse(body.task)
    },

    async listBranches(repo?: string): Promise<{ branches: string[]; current: string | null }> {
      const query = repo === undefined ? '' : `?repo=${encodeURIComponent(repo)}`
      const body = asRecord(await request('GET', `/git/branches${query}`))
      return {
        branches: stringList(body.branches),
        current: typeof body.current === 'string' ? body.current : null,
      }
    },

    async stageAction(
      taskId: string,
      action: StageAction,
      input: StageActionInput = {},
    ): Promise<Task> {
      const body = asRecord(
        await request('POST', `/tasks/${encodeURIComponent(taskId)}/stage`, { action, ...input }),
      )
      return taskSchema.parse(body.task)
    },

    async setCi(taskId: string, stage: string, on: boolean): Promise<Task> {
      const body = asRecord(
        await request('PUT', `/tasks/${encodeURIComponent(taskId)}/ci`, { stage, on }),
      )
      return taskSchema.parse(body.task)
    },

    async setWorkflow(taskId: string, stage: string, on: boolean): Promise<Task> {
      const body = asRecord(
        await request('PUT', `/tasks/${encodeURIComponent(taskId)}/workflow`, { stage, on }),
      )
      return taskSchema.parse(body.task)
    },

    async listStageFiles(taskId: string): Promise<StageFile[]> {
      const body = asRecord(await request('GET', `/tasks/${encodeURIComponent(taskId)}/files`))
      if (!Array.isArray(body.files)) {
        throw new Error('The daemon sent an answer this dashboard does not understand.')
      }
      return body.files.map((entry) => {
        const file = asRecord(entry)
        return {
          name: String(file.name),
          size: Number(file.size),
          modifiedAt: String(file.modifiedAt),
        }
      })
    },

    async listAttachments(taskId: string): Promise<Attachment[]> {
      const path = `/tasks/${encodeURIComponent(taskId)}/attachments`
      const body = asRecord(await request('GET', path))
      if (!Array.isArray(body.attachments)) {
        throw new Error('The daemon sent an answer this dashboard does not understand.')
      }
      return body.attachments.map((entry) => attachmentSchema.parse(entry))
    },

    async readAttachment(taskId: string, name: string): Promise<Blob> {
      const id = encodeURIComponent(taskId)
      return requestBlob(`/tasks/${id}/attachments/${encodeURIComponent(name)}`)
    },

    async readStageFile(taskId: string, name: string): Promise<string> {
      const body = asRecord(
        await request(
          'GET',
          `/tasks/${encodeURIComponent(taskId)}/files/${encodeURIComponent(name)}`,
        ),
      )
      return String(body.text ?? '')
    },

    async writeStageFile(taskId: string, name: string, text: string): Promise<void> {
      await request(
        'PUT',
        `/tasks/${encodeURIComponent(taskId)}/files/${encodeURIComponent(name)}`,
        {
          text,
        },
      )
    },

    async createIssue(taskId: string): Promise<Task> {
      const body = asRecord(await request('POST', `/tasks/${encodeURIComponent(taskId)}/issue`))
      return taskSchema.parse(body.task)
    },

    async getSettings(repo: string): Promise<LoadedSettings> {
      const body = asRecord(await request('GET', `/repos/${encodeURIComponent(repo)}/settings`))
      return {
        settings: projectSettingsSchema.parse(body.settings),
        error: typeof body.error === 'string' ? body.error : null,
        found: body.found === true,
        path: String(body.path ?? ''),
        text: typeof body.text === 'string' ? body.text : null,
        version: typeof body.version === 'string' ? body.version : null,
      }
    },

    // With `version`, the daemon answers 409 (see isConflict) when the file changed since then.
    async saveSettings(repo: string, raw: unknown, version?: string): Promise<SavedSettings> {
      const body = asRecord(
        await request(
          'PUT',
          `/repos/${encodeURIComponent(repo)}/settings`,
          raw,
          version === undefined ? {} : { 'if-match': version },
        ),
      )
      return {
        settings: projectSettingsSchema.parse(body.settings),
        version: typeof body.version === 'string' ? body.version : null,
      }
    },

    // The skills of every agent for a repository, for the skill picker of the stage prompts.
    async getSkills(repo: string): Promise<SkillList> {
      const body = asRecord(await request('GET', `/repos/${encodeURIComponent(repo)}/skills`))
      return {
        skills: Array.isArray(body.skills) ? body.skills.map(skillInfo) : [],
        notices: Array.isArray(body.notices) ? stringList(body.notices) : [],
      }
    },

    async deleteTask(taskId: string): Promise<{ id: string; tasks: Task[] }> {
      const body = asRecord(await request('DELETE', `/tasks/${encodeURIComponent(taskId)}`))
      const listed = Array.isArray(body.tasks) ? body.tasks : []
      const tasks = listed.map((task) => taskSchema.parse(task))
      return { id: typeof body.id === 'string' ? body.id : taskId, tasks }
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
