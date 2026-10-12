import { stageName, type Task, type TaskState, type TerminalLane } from '@natamux/core'
import { terminalState } from './panes'
import { stageLabel } from './tasks'

export const FOLDS_KEY = 'natamux.sidebar-folds'
// Old groups and tasks do not need a remembered fold forever.
const MAX_FOLDS = 300

// Which repository groups, finished lists and task rows the user opened (true) or closed (false).
export function parseFolds(raw: string | null | undefined): Record<string, boolean> {
  try {
    const value: unknown = JSON.parse(raw ?? '{}')
    if (typeof value !== 'object' || value === null || Array.isArray(value)) {
      return {}
    }
    return Object.fromEntries(Object.entries(value).filter(([, open]) => typeof open === 'boolean'))
  } catch {
    return {}
  }
}

export function withFold(
  folds: Readonly<Record<string, boolean>>,
  key: string,
  open: boolean,
): Record<string, boolean> {
  const { [key]: _old, ...rest } = folds
  return Object.fromEntries(Object.entries({ ...rest, [key]: open }).slice(-MAX_FOLDS))
}

export function isFoldOpen(
  folds: Readonly<Record<string, boolean>>,
  key: string,
  fallback: boolean,
): boolean {
  return folds[key] ?? fallback
}

// The order of the most urgent first, as the Needs you section reads it.
const URGENT: readonly TaskState[] = [
  'needs-input',
  'failed',
  'blocked',
  'review',
  'running',
  'ready',
]

// The state a folded row shows for its children, so a failed or waiting child is never hidden.
export function urgentState(states: readonly TaskState[]): TaskState | null {
  return URGENT.find((state) => states.includes(state)) ?? null
}

// A row opens by itself while one of its children works, waits or failed.
export function opensByDefault(states: readonly TaskState[]): boolean {
  return states.some(
    (state) => state === 'running' || state === 'needs-input' || state === 'failed',
  )
}

// "Step 3 of 5", from the steps the task has. A group of parallel stages is one step.
export function stagePosition(
  task: Pick<Task, 'stage' | 'stages' | 'state'>,
): { at: number; of: number } | null {
  if (task.stages.length < 2 || stageLabel(task) === null) {
    return null
  }
  const index = task.stages.findIndex((step) =>
    typeof step === 'string'
      ? step === task.stage
      : step.id === task.stage || step.parallel.includes(task.stage),
  )
  return index === -1 ? null : { at: index + 1, of: task.stages.length }
}

export function subtaskCount(
  id: string,
  tasks: readonly Pick<Task, 'parentId' | 'state'>[],
): { done: number; total: number } | null {
  const own = tasks.filter((task) => task.parentId === id)
  if (own.length === 0) {
    return null
  }
  return { done: own.filter((task) => task.state === 'done').length, total: own.length }
}

// The filtered tasks plus the parent of each subtask among them, so a match is shown in its place.
export function withParents(shown: readonly Task[], all: readonly Task[]): Task[] {
  const result = [...shown]
  for (const task of shown) {
    const parent = all.find((other) => other.id === task.parentId)
    if (parent !== undefined && parent.workspace !== null && !result.includes(parent)) {
      result.push(parent)
    }
  }
  return result
}

export type AgentChild = {
  key: string
  // "Shell 2", or the name of a parallel stage that has no terminal yet.
  name: string
  // The stage a shell works on, by name. Null for a stage row, whose name is the stage.
  stage: string | null
  state: TaskState
  // The terminal to focus. Null: the task's first shell.
  shell: string | null
}

// The agents of a task: its agent terminals, then the parallel stages no terminal works on yet.
export function agentChildren(
  task: Pick<Task, 'stage' | 'state' | 'lanes'>,
  terminals: Readonly<Record<string, TerminalLane>> = {},
): AgentChild[] {
  const shells = Object.entries(terminals)
    .sort(([a], [b]) => Number(a) - Number(b) || a.localeCompare(b))
    .map(([name, lane]) => ({
      key: `shell:${name}`,
      name: `Shell ${name}`,
      stage: lane.stage === null ? null : stageName({ id: lane.stage }),
      state: terminalState(task, lane) ?? task.state,
      shell: name,
    }))
  const covered = new Set(Object.values(terminals).map((lane) => lane.stage))
  const lanes = Object.entries(task.lanes)
    .filter(([stage]) => !covered.has(stage))
    .map(([stage, state]) => ({
      key: `lane:${stage}`,
      name: stageName({ id: stage }),
      stage: null,
      state,
      shell: null,
    }))
  return [...shells, ...lanes]
}
