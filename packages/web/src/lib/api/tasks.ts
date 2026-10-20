import {
  type DiffStat,
  diffStatSchema,
  type EndedShell,
  type EventType,
  endedShellSchema,
  type Task,
  type TaskTemplate,
  type TaskUsage,
  taskEventSchema,
  taskSchema,
  taskTemplateSchema,
  taskUsageSchema,
} from '@natamux/core'
import type { Client } from './client'
import { asRecord, isPreparingStep, readJobs, readTaskActivity } from './parse'
import type {
  AgentActivity,
  Job,
  NewWorkspaceInput,
  PreparingStep,
  RemoveWorkspaceResult,
  SendEventResult,
  StoredEvent,
  TaskChanges,
} from './types'

export function tasksApi(client: Client) {
  const { request } = client

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

    // A task, its worktree and, when one is named, its agent, in one step. The answer comes before
    // the worktree is made: see listPreparing.
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

    // Starts the agent of Terminal 1 of a done or cancelled task again, on its last session.
    async reopenAgent(taskId: string): Promise<Task> {
      const body = asRecord(
        await request('POST', `/tasks/${encodeURIComponent(taskId)}/agent/reopen`),
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

    async listJobs(): Promise<Job[]> {
      return readJobs(asRecord(await request('GET', '/jobs')).jobs)
    },

    async listActivity(): Promise<AgentActivity> {
      const activity = asRecord(asRecord(await request('GET', '/activity')).activity)
      const result: AgentActivity = { byTask: {}, byTerminal: {} }
      for (const [taskId, terminals] of Object.entries(activity)) {
        const { lines, latest } = readTaskActivity(terminals)
        if (latest !== null) {
          result.byTask[taskId] = latest
        }
        result.byTerminal[taskId] = lines
      }
      return result
    },

    // The tokens and estimated cost of every task that has usage, by task id.
    async listUsage(): Promise<Record<string, TaskUsage>> {
      const usage = asRecord(asRecord(await request('GET', '/usage')).usage)
      return Object.fromEntries(
        Object.entries(usage).map(([taskId, item]) => [taskId, taskUsageSchema.parse(item)]),
      )
    },

    // The workspaces being made, by task id. A step this dashboard does not know is left out.
    async listPreparing(): Promise<Record<string, PreparingStep>> {
      const preparing = asRecord(asRecord(await request('GET', '/workspaces/preparing')).preparing)
      const result: Record<string, PreparingStep> = {}
      for (const [taskId, step] of Object.entries(preparing)) {
        if (isPreparingStep(step)) {
          result[taskId] = step
        }
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

    // The last screens of the shells of a done or cancelled task.
    async listEndedShells(taskId: string): Promise<EndedShell[]> {
      const path = `/tasks/${encodeURIComponent(taskId)}/ended-shells`
      const body = asRecord(await request('GET', path))
      if (!Array.isArray(body.shells)) {
        throw new Error('The daemon sent an answer this dashboard does not understand.')
      }
      return body.shells.map((entry) => endedShellSchema.parse(entry))
    },

    async createIssue(taskId: string): Promise<Task> {
      const body = asRecord(await request('POST', `/tasks/${encodeURIComponent(taskId)}/issue`))
      return taskSchema.parse(body.task)
    },

    async deleteTask(taskId: string): Promise<{ id: string; tasks: Task[] }> {
      const body = asRecord(await request('DELETE', `/tasks/${encodeURIComponent(taskId)}`))
      const listed = Array.isArray(body.tasks) ? body.tasks : []
      const tasks = listed.map((task) => taskSchema.parse(task))
      return { id: typeof body.id === 'string' ? body.id : taskId, tasks }
    },

    // A merged branch goes too, unless `deleteBranch` is false. The daemon keeps any other.
    async removeWorkspace(taskId: string, deleteBranch = true): Promise<RemoveWorkspaceResult> {
      const path = `/tasks/${encodeURIComponent(taskId)}/workspace?deleteBranch=${deleteBranch}`
      const body = asRecord(await request('DELETE', path))
      return { task: taskSchema.parse(body.task), branchDeleted: body.branchDeleted === true }
    },
  }
}
