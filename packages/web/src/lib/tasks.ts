import { type EventType, needsMe, TASK_STATES, type Task, type TaskState } from '@helmsman/core'

export const STATE_LABEL: Record<TaskState, string> = {
  draft: 'Draft',
  ready: 'Ready',
  running: 'Running',
  'needs-input': 'Needs input',
  review: 'In review',
  blocked: 'Blocked',
  failed: 'Failed',
  done: 'Done',
  cancelled: 'Cancelled',
}

export type TaskAction = {
  label: string
  type: EventType
  hotkey?: 'a' | 'r'
  confirm?: boolean
}

const STATE_ACTIONS: Record<TaskState, readonly TaskAction[]> = {
  draft: [{ label: 'Mark ready', type: 'task.readied' }],
  ready: [{ label: 'Block', type: 'task.blocked' }],
  running: [{ label: 'Block', type: 'task.blocked' }],
  'needs-input': [{ label: 'Resume', type: 'agent.resumed' }],
  review: [
    { label: 'Approve', type: 'review.approved', hotkey: 'a' },
    { label: 'Request changes', type: 'review.changes-requested', hotkey: 'r' },
  ],
  blocked: [{ label: 'Unblock', type: 'task.unblocked' }],
  failed: [{ label: 'Retry', type: 'task.retried' }],
  done: [],
  cancelled: [],
}

const CANCEL_ACTION: TaskAction = { label: 'Cancel task', type: 'task.cancelled', confirm: true }

export function actionsFor(state: TaskState): TaskAction[] {
  const isFinished = state === 'done' || state === 'cancelled'
  return isFinished ? [] : [...STATE_ACTIONS[state], CANCEL_ACTION]
}

export function sortByRecent(tasks: readonly Task[]): Task[] {
  return [...tasks].sort(
    (a, b) => b.updatedAt.localeCompare(a.updatedAt) || a.id.localeCompare(b.id),
  )
}

export function upsertTask(tasks: readonly Task[], task: Task): Task[] {
  return sortByRecent([...tasks.filter((existing) => existing.id !== task.id), task])
}

// A running task that has shown no sign of life for this long is probably stuck, interrupted or
// closed. Ten minutes: a long command can keep an agent silent for a while without a problem.
export const QUIET_AFTER_MS = 10 * 60 * 1000

export function isQuiet(task: Task, now: Date): boolean {
  return task.state === 'running' && now.getTime() - Date.parse(task.updatedAt) >= QUIET_AFTER_MS
}

// What the agent button of a task does. While the agent works or waits there is nothing to start.
// After it ended or failed, starting it again is a restart.
export function agentAction(task: Pick<Task, 'state' | 'agent'>): 'start' | 'restart' | null {
  if (task.state === 'running' || task.state === 'needs-input') {
    return null
  }
  return task.agent === null ? 'start' : 'restart'
}

// Tasks that need the user: waiting on them, or running but quiet. The longest wait comes first.
export function needsMeTasks(tasks: readonly Task[], now: Date): Task[] {
  return tasks
    .filter((task) => needsMe(task.state) || isQuiet(task, now))
    .sort((a, b) => a.updatedAt.localeCompare(b.updatedAt) || a.id.localeCompare(b.id))
}

export function countByState(tasks: readonly Task[]): Record<TaskState, number> {
  const counts = Object.fromEntries(TASK_STATES.map((state) => [state, 0])) as Record<
    TaskState,
    number
  >
  for (const task of tasks) {
    counts[task.state] += 1
  }
  return counts
}

export function filterTasks(
  tasks: readonly Task[],
  stateFilter: TaskState | null,
  search: string,
): Task[] {
  const needle = search.trim().toLowerCase()
  return tasks.filter((task) => {
    if (stateFilter !== null && task.state !== stateFilter) {
      return false
    }
    if (needle === '') {
      return true
    }
    const haystack = [task.id, task.title, task.agent, task.branch, ...task.labels]
      .filter((part): part is string => part !== null)
      .join(' ')
      .toLowerCase()
    return haystack.includes(needle)
  })
}

export function formatAge(isoTime: string, now: Date): string {
  const seconds = Math.max(0, Math.floor((now.getTime() - new Date(isoTime).getTime()) / 1000))
  if (seconds < 60) {
    return 'just now'
  }
  if (seconds < 3600) {
    return `${Math.floor(seconds / 60)}m ago`
  }
  if (seconds < 86400) {
    return `${Math.floor(seconds / 3600)}h ago`
  }
  return `${Math.floor(seconds / 86400)}d ago`
}
