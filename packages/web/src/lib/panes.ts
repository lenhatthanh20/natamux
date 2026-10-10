import type { Task, TaskState } from '@natamux/core'
import { STATE_LABEL } from './tasks'

// More panes than this make each terminal too small to read.
export const MAX_PANES = 6

function hasAgent(task: Task): boolean {
  return task.workspace !== null && (task.state === 'running' || task.state === 'needs-input')
}

// The tasks in the agents grid. Until the user picks, the ones whose agent works or waits.
export function agentPaneTasks(chosen: readonly string[] | null, tasks: readonly Task[]): Task[] {
  if (chosen === null) {
    return tasks.filter(hasAgent).slice(0, MAX_PANES)
  }
  return chosen
    .map((id) => tasks.find((task) => task.id === id))
    .filter((task): task is Task => task !== undefined && task.workspace !== null)
}

// How many columns the pane grid has. Auto fits as many as the width allows.
export type PaneLayout = 'auto' | '1' | '2' | '4'
export const PANE_LAYOUTS: readonly PaneLayout[] = ['1', '2', '4', 'auto']

export function parsePaneLayout(raw: string | null | undefined): PaneLayout {
  return PANE_LAYOUTS.find((layout) => layout === raw) ?? 'auto'
}

const MIN_PANE_WIDTH = 320
const PANE_GAP = 8
// Before the grid is measured (and in tests), three columns are assumed.
const UNMEASURED_COLUMNS = 3

// The most columns that keep every pane at its minimum width.
export function maxColumns(width: number | null): number {
  if (width === null || width <= 0) {
    return UNMEASURED_COLUMNS
  }
  return Math.max(1, Math.floor((width + PANE_GAP) / (MIN_PANE_WIDTH + PANE_GAP)))
}

// A layout that needs more room than there is gets the most that fits.
export function columnsFor(layout: PaneLayout, count: number, width: number | null): number {
  const fits = maxColumns(width)
  if (layout === 'auto') {
    // Balanced rows: five panes in a grid of four columns become 3 + 2, not 4 + 1.
    const rows = Math.ceil(Math.max(count, 1) / fits)
    return Math.max(1, Math.ceil(Math.max(count, 1) / rows))
  }
  return Math.max(1, Math.min(Number(layout), fits, Math.max(count, 1)))
}

// The last pane takes the rest of its row, so the grid has no hole.
export function paneSpan(index: number, count: number, columns: number): number {
  if (index !== count - 1) {
    return 1
  }
  return columns - ((count - 1) % columns)
}

// The line under a pane's title: the state, and what the agent does now.
export function paneStateText(state: TaskState, activity: string | null | undefined): string {
  if (state === 'needs-input') {
    return 'Needs your input'
  }
  const label = STATE_LABEL[state]
  return state === 'running' && activity ? `${label} · ${activity}` : label
}
