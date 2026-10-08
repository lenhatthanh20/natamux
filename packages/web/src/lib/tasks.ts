import {
  type DiffStat,
  type EventType,
  needsMe,
  TASK_STATES,
  type Task,
  type TaskState,
} from '@helmsman/core'

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

// Newest or longest-waiting first is not a state, so states have a fixed order: what needs the user
// comes first, finished work last. It is the default sort and the order of the filter chips.
export const STATE_ORDER: readonly TaskState[] = [
  'needs-input',
  'review',
  'failed',
  'blocked',
  'running',
  'ready',
  'draft',
  'done',
  'cancelled',
]

export type SortKey = 'id' | 'task' | 'state' | 'agent' | 'branch' | 'changes' | 'pr' | 'activity'
export type SortDirection = 'asc' | 'desc'
// Null is the default order: state group, then id descending.
export type TaskSort = { key: SortKey; direction: SortDirection } | null

// "T-0042" -> 42, so T-10000 sorts after T-9999.
export function taskNumber(id: string): number {
  const value = Number.parseInt(id.replace(/^\D+/, ''), 10)
  return Number.isNaN(value) ? 0 : value
}

function byId(a: Task, b: Task): number {
  return taskNumber(b.id) - taskNumber(a.id) || b.id.localeCompare(a.id)
}

function pullRequestOrder(task: Task): number | null {
  const match = task.pr === null ? null : /(\d+)\/?$/.exec(task.pr)
  return match === null || match[1] === undefined ? null : Number(match[1])
}

// What a task is worth under a sort key. Null means it has no value, and such tasks always go last.
function sortValue(
  task: Task,
  key: SortKey,
  diffStats: Readonly<Record<string, DiffStat>>,
): string | number | null {
  switch (key) {
    case 'id':
      return taskNumber(task.id)
    case 'task':
      return task.title.toLowerCase()
    case 'state':
      return STATE_ORDER.indexOf(task.state)
    case 'agent':
      return task.agent
    case 'branch':
      return task.branch
    case 'changes': {
      const stat = diffStats[task.id]
      return stat === undefined || stat.files === 0 ? null : stat.added + stat.removed
    }
    case 'pr':
      return pullRequestOrder(task)
    case 'activity':
      return task.updatedAt
  }
}

// The same tasks in a stable order. Ties always fall back to the default order, so two rows never
// swap places just because a value is equal.
export function sortTasks(
  tasks: readonly Task[],
  sort: TaskSort,
  diffStats: Readonly<Record<string, DiffStat>> = {},
): Task[] {
  const byDefault = (a: Task, b: Task) =>
    STATE_ORDER.indexOf(a.state) - STATE_ORDER.indexOf(b.state) || byId(a, b)
  if (sort === null) {
    return [...tasks].sort(byDefault)
  }
  const sign = sort.direction === 'asc' ? 1 : -1
  return [...tasks].sort((a, b) => {
    const left = sortValue(a, sort.key, diffStats)
    const right = sortValue(b, sort.key, diffStats)
    if (left === null || right === null) {
      return left === right ? byDefault(a, b) : left === null ? 1 : -1
    }
    const order = left < right ? -1 : left > right ? 1 : 0
    return order * sign || byDefault(a, b)
  })
}

// A click on a column header: first ascending, then descending, then the default order again.
export function nextSort(current: TaskSort, key: SortKey): TaskSort {
  if (current === null || current.key !== key) {
    return { key, direction: 'asc' }
  }
  return current.direction === 'asc' ? { key, direction: 'desc' } : null
}

export type NeedsMeReason = 'asks' | 'review' | 'failed' | 'blocked' | 'quiet'

// The one word that says why a task is in the "needs you" list.
export function needsMeReason(task: Task, now: Date): NeedsMeReason {
  if (isQuiet(task, now)) {
    return 'quiet'
  }
  switch (task.state) {
    case 'review':
      return 'review'
    case 'failed':
      return 'failed'
    case 'blocked':
      return 'blocked'
    default:
      return 'asks'
  }
}

export type AgeStyle = 'short' | 'long'

const WEEK_SECONDS = 7 * 86400

// One time format for the whole app. Short is for rows and tables: now, 4m, 3h, 2d, then a date.
// Long is for sentences: just now, 4m ago, 3h ago, 2d ago, then a date.
export function formatAge(isoTime: string, now: Date, style: AgeStyle = 'short'): string {
  const then = new Date(isoTime)
  const seconds = Math.max(0, Math.floor((now.getTime() - then.getTime()) / 1000))
  const long = style === 'long'
  if (seconds < 60) {
    return long ? 'just now' : 'now'
  }
  if (seconds >= WEEK_SECONDS) {
    const sameYear = then.getFullYear() === now.getFullYear()
    return then.toLocaleDateString('en-US', {
      month: 'short',
      day: 'numeric',
      ...(sameYear ? {} : { year: 'numeric' }),
    })
  }
  const [amount, unit] =
    seconds < 3600
      ? [Math.floor(seconds / 60), 'm']
      : seconds < 86400
        ? [Math.floor(seconds / 3600), 'h']
        : [Math.floor(seconds / 86400), 'd']
  return long ? `${amount}${unit} ago` : `${amount}${unit}`
}

// The exact local date and time, for the tooltip of a relative time.
export function formatExact(isoTime: string): string {
  return new Date(isoTime).toLocaleString('en-US', { dateStyle: 'medium', timeStyle: 'medium' })
}

// What a screen reader says for a task row: "Fix token refresh race, Needs input, T-0008, 34m ago".
export function rowLabel(task: Task, now: Date): string {
  return `${task.title}, ${STATE_LABEL[task.state]}, ${task.id}, ${formatAge(task.updatedAt, now, 'long')}`
}
