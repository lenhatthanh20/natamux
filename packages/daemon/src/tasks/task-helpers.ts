import {
  DEFAULT_PROJECT_SETTINGS,
  LEGACY_PIPELINE,
  type ProjectSettings,
  type StagePosition,
  type Task,
  type TaskEvent,
  type TaskState,
} from '@natamux/core'

// How often a running task's sign of life is written down at most.
export const ACTIVITY_INTERVAL_MS = 30_000

// How often a sign of life alone rewrites the task file at most. The index gets every one.
export const ACTIVITY_FILE_INTERVAL_MS = 60_000

// How often a read looks at the task files at most, for an edit by hand that the watcher missed:
// some file systems give no events. getTask stats one file; listTasks reads the folder.
export const STAT_CHECK_MS = 1000
export const LIST_CHECK_MS = 2000

export type SettingsOf = (repo: string | null) => ProjectSettings

// A done or cancelled task needs its shells no more. Neither does a task that is gone.
export function isTaskEnded(
  tasks: { getTask(taskId: string): Pick<Task, 'state'> },
  taskId: string,
): boolean {
  try {
    const { state } = tasks.getTask(taskId)
    return state === 'done' || state === 'cancelled'
  } catch {
    return true
  }
}

// The flow tasks had before stages: implement, review, release. The daemon passes the settings of
// each repository instead.
export const LEGACY_SETTINGS: ProjectSettings = {
  ...DEFAULT_PROJECT_SETTINGS,
  stages: LEGACY_PIPELINE,
}

// Compares as SQLite compares text, so the order stays the one the index gave.
export function compareText(a: string, b: string): number {
  if (a === b) {
    return 0
  }
  return a < b ? -1 : 1
}

export function positionOf(task: Pick<Task, 'stage' | 'state' | 'lanes' | 'paths'>): StagePosition {
  return { stage: task.stage, state: task.state, lanes: task.lanes, paths: task.paths }
}

// The task at its new position. Entering a new step starts its clock.
export function moveTask(task: Task, position: StagePosition, moved: boolean, at: string): Task {
  return {
    ...task,
    stage: position.stage,
    state: position.state,
    lanes: position.lanes,
    paths: position.paths ?? task.paths,
    stageSince: moved ? at : task.stageSince,
  }
}

// An agent waits for its background work from its first `agent.waiting` until it shows up again,
// or the task leaves running. Other events, such as checks that pass, do not end the wait.
export function backgroundSinceAfter(
  task: Task,
  event: TaskEvent,
  state: TaskState,
): string | null {
  if (event.type === 'agent.waiting') {
    return task.backgroundSince ?? event.at
  }
  return state !== 'running' || event.source === 'agent' ? null : task.backgroundSince
}
