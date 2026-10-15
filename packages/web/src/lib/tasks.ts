import {
  type DiffStat,
  type EventType,
  needsMe,
  stageName,
  TASK_STATES,
  type Task,
  type TaskContext,
  type TaskState,
} from '@natamux/core'

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

// Each stage of a group with its own state, for the tooltip of the task state. The task state is
// the most urgent of these, so this shows what the other stages do.
export function laneSummary(task: Pick<Task, 'lanes'>): string | undefined {
  const lanes = Object.entries(task.lanes)
  if (lanes.length === 0) {
    return undefined
  }
  return lanes.map(([id, state]) => `${stageName({ id })}: ${STATE_LABEL[state]}`).join(' · ')
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

// What the delete confirmation says. A workspace goes with the task; a branch with unmerged
// commits stays, the same rule as removing a workspace.
export function deleteTaskDescription(task: Pick<Task, 'workspace'>): string {
  const gone = 'This deletes the task, its notes and its history.'
  if (task.workspace === null) {
    return `${gone} This cannot be undone.`
  }
  return (
    `${gone} The worktree folder goes with it. ` +
    'The branch stays when it has commits that are not merged.'
  )
}

// What a cancel stops and what it keeps.
export function cancelTaskDescription(task: Pick<Task, 'workspace'>): string {
  const final = 'A cancelled task cannot be started again.'
  if (task.workspace === null) {
    return final
  }
  const stops = 'The agent and its shells stop now.'
  const stays = 'The commits, the branch and the workspace stay.'
  return `${stops} ${stays} ${final}`
}

const byRecent = (a: Task, b: Task) =>
  b.updatedAt.localeCompare(a.updatedAt) || a.id.localeCompare(b.id)

export function sortByRecent(tasks: readonly Task[]): Task[] {
  return [...tasks].sort(byRecent)
}

// A late answer to a request must not replace what the live feed already brought: keep the task
// that was updated last. The same time counts as newer, so the answer of an edit still shows.
// The daemon sends the list newest first, so the task goes into its place without a full sort.
export function upsertTask(tasks: readonly Task[], task: Task): Task[] {
  const known = tasks.find((existing) => existing.id === task.id)
  if (known !== undefined && known.updatedAt > task.updatedAt) {
    return [...tasks]
  }
  const next = tasks.filter((existing) => existing.id !== task.id)
  const at = next.findIndex((existing) => byRecent(task, existing) < 0)
  next.splice(at === -1 ? next.length : at, 0, task)
  return next
}

// A change from the live feed: a task that changed, or the id of one that was deleted.
export type TaskChange = Task | { deleted: string }

// The changes in the order they came.
export function applyTaskChanges(tasks: readonly Task[], changes: readonly TaskChange[]): Task[] {
  return changes.reduce<Task[]>(
    (list, change) =>
      'deleted' in change
        ? list.filter((task) => task.id !== change.deleted)
        : upsertTask(list, change),
    [...tasks],
  )
}

// A running task that has shown no sign of life for this long is probably stuck, interrupted or
// closed. Ten minutes: a long command can keep an agent silent for a while without a problem.
export const QUIET_AFTER_MS = 10 * 60 * 1000

// An agent that waits for its background work, such as a CI watch, is woken by it, so it is not
// stuck however long the work takes.
export function isQuiet(task: Task, now: Date): boolean {
  return (
    task.state === 'running' &&
    task.backgroundSince === null &&
    now.getTime() - Date.parse(task.updatedAt) >= QUIET_AFTER_MS
  )
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

// What a screen reader says for a task row, such as
// "Fix token refresh race, Needs input, T-0008, 34m ago".
// The stage a task is in, such as "Plan". Null while it has none: a draft, or a finished task.
export function stageLabel(task: Pick<Task, 'stage' | 'state'>): string | null {
  if (task.state === 'draft' || task.state === 'done' || task.state === 'cancelled') {
    return null
  }
  return stageName({ id: task.stage })
}

// "Plan · Needs input": the stage and its state in one text.
export function stageStateLabel(task: Pick<Task, 'stage' | 'state'>): string {
  const stage = stageLabel(task)
  return stage === null ? STATE_LABEL[task.state] : `${stage} · ${STATE_LABEL[task.state]}`
}

export function rowLabel(task: Task, now: Date): string {
  return `${task.title}, ${STATE_LABEL[task.state]}, ${task.id}, ${formatAge(task.updatedAt, now, 'long')}`
}

const SOURCE_KIND: Record<TaskContext['source'], string> = {
  manual: 'My notes',
  'github-issue': 'GitHub issue',
  skill: 'skill',
  mcp: 'MCP server',
}

// "linear (skill) LIN-7": the source by its name when it has one, as the Start a task dialog
// lists it, then the reference.
export function contextText(context: TaskContext): string {
  const kind = SOURCE_KIND[context.source]
  const name = context.skill ?? context.name
  const source = name === undefined ? kind : `${name} (${kind})`
  return context.ref === '' ? source : `${source} ${context.ref}`
}
