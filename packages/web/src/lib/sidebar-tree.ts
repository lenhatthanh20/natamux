import { stageName, type Task, type TaskState } from '@natamux/core'
import { agentLabel } from './agents'
import type { TaskShells } from './api'
import { terminalState } from './panes'
import { STATE_LABEL, stageLabel } from './tasks'

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

// The order of the most urgent first.
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

// How much room a row takes: what needs the user is a card, what is finished is one line.
export type RowTier = 'attention' | 'live' | 'quiet'

export function rowTier(state: TaskState): RowTier {
  if (state === 'needs-input' || state === 'failed' || state === 'blocked') {
    return 'attention'
  }
  if (state === 'done' || state === 'cancelled' || state === 'draft') {
    return 'quiet'
  }
  return 'live'
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

// A row under a task: one of its shells, or a parallel stage that has no shell yet.
export type ShellRow = {
  key: string
  // The stage the agent works on, "Shell 2" for an agent on no stage, or the program of a shell
  // with no agent.
  label: string
  agent: string | null
  // Null: a shell with no agent, or an agent whose state is not known, such as one started by hand
  // before the daemon restarted.
  state: TaskState | null
  // The terminal to focus. Null: the task's first shell.
  shell: string | null
  // A shell with no agent in it.
  plain: boolean
}

// The rows under a task: each open shell in shell order, then the parallel stages no shell works
// on yet. Before the shells are read, only the stages.
export function shellRows(
  task: Pick<Task, 'stage' | 'state' | 'lanes'>,
  shells?: TaskShells,
): ShellRow[] {
  const terminals = [...(shells?.terminals ?? [])].sort(
    (a, b) => Number(a) - Number(b) || a.localeCompare(b),
  )
  const rows = terminals.map((name): ShellRow => {
    const lane = shells?.lanes[name]
    if (lane === undefined) {
      const program = shells?.programs[name] ?? null
      return {
        key: `shell:${name}`,
        label: program ?? 'Shell',
        agent: null,
        state: null,
        shell: name,
        plain: true,
      }
    }
    return {
      key: `shell:${name}`,
      label: lane.stage === null ? `Shell ${name}` : stageName({ id: lane.stage }),
      agent: lane.agent ?? null,
      state: terminalState(task, lane),
      shell: name,
      plain: false,
    }
  })
  const covered = new Set(Object.values(shells?.lanes ?? {}).map((lane) => lane.stage))
  const lanes = Object.entries(task.lanes)
    .filter(([stage]) => !covered.has(stage))
    .map(([stage, state]) => ({
      key: `lane:${stage}`,
      label: stageName({ id: stage }),
      agent: null,
      state,
      shell: null,
      plain: false,
    }))
  return [...rows, ...lanes]
}

// "Implement, Claude Code, Shell 1, Running"; "Shell 2, zsh" for a shell with no agent.
export function shellRowName(row: ShellRow): string {
  if (row.plain) {
    return `Shell ${row.shell}, ${row.label}`
  }
  const named = row.shell === null || row.label === `Shell ${row.shell}`
  const shell = named ? null : `Shell ${row.shell}`
  const agent = row.agent === null ? null : agentLabel(row.agent)
  const state = row.state === null ? 'State not known' : STATE_LABEL[row.state]
  return [row.label, agent, shell, state].filter((part) => part !== null).join(', ')
}

// "S1" at the right of a row, where its name does not already say the shell.
export function shellTag(row: ShellRow): string | null {
  return row.shell === null || row.label === `Shell ${row.shell}` ? null : `S${row.shell}`
}

// The task glyph spins while any of its agents works, also when the task waits on another one.
export function taskGlyphState(task: Pick<Task, 'state' | 'lanes'>, rows: ShellRow[]): TaskState {
  const states = [...rows.map((row) => row.state), ...Object.values(task.lanes)]
  return states.includes('running') ? 'running' : task.state
}

export type Comet = {
  // From 0 to 1: the steps done, and the current one.
  progress: number
  at: number
  of: number
  // The state of the current stage, which colors the comet.
  state: TaskState
  // The current stage by name. Null for a finished task.
  stage: string | null
  // Every step and its state, for the tooltip and screen readers.
  summary: string
}

const lower = (state: TaskState) => STATE_LABEL[state].toLowerCase()

// How far a task is through its own steps. A group is one step; its most urgent stage speaks
// for it.
export function cometOf(task: Pick<Task, 'stage' | 'stages' | 'state' | 'lanes'>): Comet {
  const steps = task.stages
  const finished = task.state === 'done' || task.state === 'cancelled'
  const found = steps.findIndex((step) =>
    typeof step === 'string'
      ? step === task.stage
      : step.id === task.stage || step.parallel.includes(task.stage),
  )
  const index = finished && task.state === 'done' ? steps.length - 1 : Math.max(0, found)
  const current = steps[index]
  const lanes = current === undefined || typeof current === 'string' ? [] : current.parallel
  const open = lanes.filter((id) => task.lanes[id] !== 'done')
  const laneState = urgentState(open.map((id) => task.lanes[id] ?? 'ready'))
  const urgentLane = open.find((id) => task.lanes[id] === laneState)
  const state = finished ? task.state : (laneState ?? task.state)
  const mark = (at: number, lane: string | null): string => {
    if (at < index || task.state === 'done') {
      return 'done'
    }
    if (at > index) {
      return 'not started'
    }
    return lower(lane === null ? task.state : (task.lanes[lane] ?? task.state))
  }
  const names = (step: (typeof steps)[number], at: number) =>
    typeof step === 'string'
      ? [`${stageName({ id: step })} ${mark(at, null)}`]
      : step.parallel.map((id) => `${stageName({ id })} ${mark(at, id)}`)
  return {
    progress: steps.length === 0 || task.state === 'done' ? 1 : (index + 1) / steps.length,
    at: index + 1,
    of: steps.length,
    state,
    stage: finished
      ? null
      : stageName({ id: urgentLane ?? (typeof current === 'string' ? current : task.stage) }),
    summary: steps.flatMap(names).join(', '),
  }
}
