import { type Task, type TaskUsage, taskSchema, taskUsageSchema } from '@natamux/core'
import type { QueryClient, QueryKey } from '@tanstack/react-query'
import {
  type AgentActivity,
  isPreparingStep,
  type Job,
  type PreparingStep,
  readJobs,
  readTaskActivity,
} from './api'

export function liveSocketUrl(
  location: Pick<Location, 'protocol' | 'host'>,
  token: string,
): string {
  const scheme = location.protocol === 'https:' ? 'wss' : 'ws'
  return `${scheme}://${location.host}/api/events?token=${encodeURIComponent(token)}`
}

// A message of the live feed that the dashboard uses, read once.
export type LiveMessage =
  // `logged`: the change added a line to the task's log. A sign of life of its agent moves only
  // its time.
  | { kind: 'task'; task: Task; logged: boolean }
  | { kind: 'task.deleted'; taskId: string }
  | { kind: 'repos.changed' }
  // What the task's agents do now: the line of each terminal, and the newest line. {}: nothing.
  | {
      kind: 'activity.changed'
      taskId: string
      lines: Record<string, string>
      latest: string | null
    }
  | { kind: 'shells.changed'; taskId: string }
  | { kind: 'workspace.dirty'; taskId: string }
  // The background jobs that run now, all of them.
  | { kind: 'jobs.changed'; jobs: Job[] }
  // The step a new workspace is at. Null: it ended, with `error` when it failed.
  | { kind: 'workspace.preparing'; taskId: string; step: PreparingStep | null; error?: string }
  // The task's whole usage after it changed.
  | { kind: 'usage.changed'; taskId: string; usage: TaskUsage }
  // The daemon skipped this dashboard while it was behind, so anything may have changed.
  | { kind: 'resync' }

// The message, or undefined for one this dashboard does not use.
export function parseLiveMessage(text: string): LiveMessage | undefined {
  let message: unknown
  try {
    message = JSON.parse(text)
  } catch {
    return undefined
  }
  if (typeof message !== 'object' || message === null) {
    return undefined
  }
  if ('task' in message) {
    const parsed = taskSchema.safeParse(message.task)
    const logged = 'kind' in message && message.kind === 'task.updated'
    return parsed.success ? { kind: 'task', task: parsed.data, logged } : undefined
  }
  const { kind, taskId, activity, jobs, step, error, usage } = message as Record<string, unknown>
  const id = typeof taskId === 'string' ? taskId : null
  switch (kind) {
    case 'repos.changed':
      return { kind: 'repos.changed' }
    case 'resync':
      return { kind: 'resync' }
    case 'task.deleted':
      return id === null ? undefined : { kind: 'task.deleted', taskId: id }
    case 'shells.changed':
      return id === null ? undefined : { kind: 'shells.changed', taskId: id }
    case 'workspace.dirty':
      return id === null ? undefined : { kind: 'workspace.dirty', taskId: id }
    case 'jobs.changed':
      try {
        return { kind: 'jobs.changed', jobs: readJobs(jobs) }
      } catch {
        return undefined
      }
    case 'workspace.preparing':
      return id === null || !(step === null || isPreparingStep(step))
        ? undefined
        : {
            kind: 'workspace.preparing',
            taskId: id,
            step,
            ...(typeof error === 'string' ? { error } : {}),
          }
    case 'usage.changed': {
      const parsed = taskUsageSchema.safeParse(usage)
      return id === null || !parsed.success
        ? undefined
        : { kind: 'usage.changed', taskId: id, usage: parsed.data }
    }
    case 'activity.changed':
      try {
        return id === null
          ? undefined
          : { kind: 'activity.changed', taskId: id, ...readTaskActivity(activity) }
      } catch {
        // Not an activity this dashboard can read.
        return undefined
      }
  }
  return undefined
}

// The live feed brings the changes; a poll this slow only catches what it missed.
export const FALLBACK_POLL_MS = 60_000

export const ACTIVITY_KEY = ['activity']
export const JOBS_KEY = ['jobs']
export const DIFFSTATS_KEY = ['diffstats']
// The usage of every task that has some, by task id (GET /usage).
export const USAGE_KEY = ['usage']
// The workspaces being made, and the step of each (see listPreparing).
export const PREPARING_KEY = ['workspaces', 'preparing']
// The sizes of all tasks come in one read, and the daemon runs git for each workspace that changed.
// With busy agents some workspace always changes, so the sizes are read at most once per this time.
export const DIFFSTATS_EVERY_MS = 15_000
// The queries of a workspace's files, by task id: the Changes, History and Docs tabs.
const WORKSPACE_KEYS = ['changes', 'file-diff', 'graph', 'stage-files', 'tree', 'workspace-file']
// The queries of a task's log, by task id, and the stage files that a logged event may have
// written. Not keyed on the time of the task: a busy agent moves it every 30 seconds.
const LOG_KEYS = ['events', 'stage-events', 'stage-files', 'question-events']

// One task's activity in the cache. The other tasks keep their objects, so what shows them does
// not render again.
export function withTaskActivity(
  current: AgentActivity,
  taskId: string,
  lines: Record<string, string>,
  latest: string | null,
): AgentActivity {
  const { [taskId]: _line, ...byTask } = current.byTask
  const { [taskId]: _lines, ...byTerminal } = current.byTerminal
  return {
    byTask: latest === null ? byTask : { ...byTask, [taskId]: latest },
    byTerminal: Object.keys(lines).length === 0 ? byTerminal : { ...byTerminal, [taskId]: lines },
  }
}

// By query cache: a read of the sizes ran within DIFFSTATS_EVERY_MS. `again`: a workspace changed
// since, so the sizes are read once more when that time ends.
const sizeWaits = new WeakMap<QueryClient, { again: boolean }>()

function readSizes(client: QueryClient): void {
  const waiting = sizeWaits.get(client)
  if (waiting !== undefined) {
    waiting.again = true
    return
  }
  const wait = { again: false }
  sizeWaits.set(client, wait)
  void client.invalidateQueries({ queryKey: DIFFSTATS_KEY })
  const end = () => {
    if (!wait.again) {
      sizeWaits.delete(client)
      return
    }
    wait.again = false
    void client.invalidateQueries({ queryKey: DIFFSTATS_KEY })
    setTimeout(end, DIFFSTATS_EVERY_MS)
  }
  setTimeout(end, DIFFSTATS_EVERY_MS)
}

// Writes a batch of live messages into the query cache: the activity in one write, and each query
// that went stale read again once.
export function applyLiveMessages(client: QueryClient, messages: readonly LiveMessage[]): void {
  if (messages.some((message) => message.kind === 'resync')) {
    void client.invalidateQueries()
    return
  }
  const stale = new Map<string, QueryKey>()
  const add = (key: QueryKey) => stale.set(JSON.stringify(key), key)
  let activity = client.getQueryData<AgentActivity>(ACTIVITY_KEY)
  const before = activity
  let preparing = client.getQueryData<Record<string, PreparingStep>>(PREPARING_KEY)
  const preparingBefore = preparing
  let sizesChanged = false
  let jobs: Job[] | undefined
  let usage = client.getQueryData<Record<string, TaskUsage>>(USAGE_KEY)
  const usageBefore = usage
  for (const message of messages) {
    if (message.kind === 'jobs.changed') {
      // Each message holds the whole list, so the last one wins.
      jobs = message.jobs
    } else if (message.kind === 'activity.changed' && activity !== undefined) {
      activity = withTaskActivity(activity, message.taskId, message.lines, message.latest)
    } else if (message.kind === 'shells.changed') {
      add(['task-shells', message.taskId])
      // A pane left for a shell the daemon closed would attach again and start a new shell. A list
      // not on screen is only marked stale: ShellPanes reads it again before it attaches.
      add(['terminals', message.taskId])
      // A finished task saves the last screens of its shells before they end.
      add(['ended-shells', message.taskId])
    } else if (message.kind === 'workspace.dirty') {
      // Only the queries on screen are read again.
      for (const key of WORKSPACE_KEYS) {
        add([key, message.taskId])
      }
      sizesChanged = true
    } else if (message.kind === 'workspace.preparing' && preparing !== undefined) {
      const { [message.taskId]: _step, ...others } = preparing
      preparing = message.step === null ? others : { ...others, [message.taskId]: message.step }
    } else if (message.kind === 'usage.changed' && usage !== undefined) {
      usage = { ...usage, [message.taskId]: message.usage }
    } else if (message.kind === 'task' && message.logged) {
      for (const key of LOG_KEYS) {
        add([key, message.task.id])
      }
    }
  }
  // Before the first read there is nothing to change: that read brings the activity.
  if (activity !== before) {
    client.setQueryData<AgentActivity>(ACTIVITY_KEY, activity)
  }
  if (jobs !== undefined) {
    client.setQueryData<Job[]>(JOBS_KEY, jobs)
  }
  if (preparing !== preparingBefore) {
    client.setQueryData(PREPARING_KEY, preparing)
  }
  if (usage !== usageBefore) {
    client.setQueryData(USAGE_KEY, usage)
  }
  for (const queryKey of stale.values()) {
    void client.invalidateQueries({ queryKey })
  }
  if (sizesChanged) {
    readSizes(client)
  }
}

// WebSocket.CONNECTING. The number, so a stand-in socket in a test works too.
const CONNECTING = 0

// Closing a socket that is still connecting makes the browser log "closed before the connection
// is established". Such a socket is closed as soon as it opens, and its handlers are dropped so
// nothing reacts to it meanwhile.
export function closeSocket(socket: WebSocket): void {
  if (socket.readyState === CONNECTING) {
    socket.onmessage = null
    socket.onclose = null
    socket.onerror = null
    socket.onopen = () => socket.close()
    return
  }
  socket.close()
}
