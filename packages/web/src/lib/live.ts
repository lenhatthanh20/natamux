import { type Task, taskSchema } from '@natamux/core'
import type { QueryClient, QueryKey } from '@tanstack/react-query'
import { type AgentActivity, readTaskActivity } from './api'

export function liveSocketUrl(
  location: Pick<Location, 'protocol' | 'host'>,
  token: string,
): string {
  const scheme = location.protocol === 'https:' ? 'wss' : 'ws'
  return `${scheme}://${location.host}/api/events?token=${encodeURIComponent(token)}`
}

// A message of the live feed that the dashboard uses, read once.
export type LiveMessage =
  | { kind: 'task'; task: Task }
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
    return parsed.success ? { kind: 'task', task: parsed.data } : undefined
  }
  const { kind, taskId, activity } = message as Record<string, unknown>
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
export const DIFFSTATS_KEY = ['diffstats']
// The queries of a workspace's files, by task id: the Changes, History and Docs tabs.
const WORKSPACE_KEYS = ['changes', 'file-diff', 'graph', 'stage-files']

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
  for (const message of messages) {
    if (message.kind === 'activity.changed' && activity !== undefined) {
      activity = withTaskActivity(activity, message.taskId, message.lines, message.latest)
    } else if (message.kind === 'shells.changed') {
      add(['task-shells', message.taskId])
    } else if (message.kind === 'workspace.dirty') {
      for (const key of WORKSPACE_KEYS) {
        add([key, message.taskId])
      }
      add(DIFFSTATS_KEY)
      // A hook or quiet shell output comes with it. Neither starts nor ends a shell, but the state
      // a hook gave a shell, or the program now in front, shows in the shells.
      add(['task-shells', message.taskId])
    }
  }
  // Before the first read there is nothing to change: that read brings the activity.
  if (activity !== before) {
    client.setQueryData<AgentActivity>(ACTIVITY_KEY, activity)
  }
  for (const queryKey of stale.values()) {
    void client.invalidateQueries({ queryKey })
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
