import type { Task, TaskState } from '@natamux/core'

// `editor` shows only while a file of the worktree is open.
export type WorkspaceTab = 'changes' | 'history' | 'terminal' | 'docs' | 'editor'
export type Layout = 'table' | 'board' | 'progress'
export type DetailsView = 'progress' | 'files' | 'details'

// Old tasks do not need a remembered answer forever.
const MAX_REMEMBERED_DETAILS = 200
const MAX_REMEMBERED_SEEN = 1000
export const MAX_RECENT = 20

// Until the user picks a view, the details show the progress beside a workspace, where the user
// watches the agent, and the task itself beside the list.
export function detailsViewFor(picked: DetailsView | null, inWorkspace: boolean): DetailsView {
  return picked ?? (inWorkspace ? 'progress' : 'details')
}

// Until the user opens or closes them, the details are open where they dock beside a workspace,
// so its progress is in sight. As a sheet or a page they would cover the terminal.
export function detailsOpenFor(choice: boolean | undefined, dockedBesideWorkspace: boolean) {
  return choice ?? dockedBesideWorkspace
}

// The workspaces that were opened, newest first, each once.
export function withRecent(current: readonly string[], taskId: string): string[] {
  return [taskId, ...current.filter((id) => id !== taskId)].slice(0, MAX_RECENT)
}

// Sets one task's entry. The newest answers stay when there are too many.
function withEntry<T>(
  current: Readonly<Record<string, T>>,
  taskId: string,
  value: T,
  max = MAX_REMEMBERED_DETAILS,
) {
  const { [taskId]: _old, ...rest } = current
  const entries = Object.entries({ ...rest, [taskId]: value })
  return Object.fromEntries(entries.slice(-max))
}

export function withDetailOpen(
  current: Readonly<Record<string, boolean>>,
  taskId: string,
  open: boolean,
): Record<string, boolean> {
  return withEntry(current, taskId, open)
}

export function withTab(
  current: Readonly<Record<string, WorkspaceTab>>,
  taskId: string,
  tab: WorkspaceTab,
): Record<string, WorkspaceTab> {
  return withEntry(current, taskId, tab)
}

// A task is unseen when its state is not the one it had when the user last looked at it. A task
// that was never recorded counts as seen: the first visit must not mark everything.
export function isUnseen(
  seen: Readonly<Record<string, TaskState>>,
  task: Pick<Task, 'id' | 'state'>,
) {
  const state = seen[task.id]
  return state !== undefined && state !== task.state
}

// Records the state the user has seen. Tasks that are recorded already keep their entry unless
// `overwrite` is set (the user opened that task just now).
export function withSeen(
  current: Readonly<Record<string, TaskState>>,
  tasks: readonly Pick<Task, 'id' | 'state'>[],
  overwrite: boolean,
): Record<string, TaskState> {
  let next = { ...current }
  for (const task of tasks) {
    if (overwrite || next[task.id] === undefined) {
      next = withEntry(next, task.id, task.state, MAX_REMEMBERED_SEEN)
    }
  }
  return next
}
