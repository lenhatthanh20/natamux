import type { SortKey } from '../../lib/tasks'

export type ColumnKey = SortKey

export type TableLayout =
  | { kind: 'cards' }
  | { kind: 'table'; columns: ColumnKey[]; widths: Record<string, number | null>; minWidth: number }

// The width of each column in px. Task has none: it takes what is left, and never less than
// TASK_MIN. When the main area is narrower than the sum, the table scrolls sideways.
const WIDTH: Record<ColumnKey, number | null> = {
  id: 72,
  task: null,
  state: 128,
  agent: 120,
  branch: 200,
  changes: 88,
  pr: 120,
  cost: 88,
  activity: 104,
}

export const TASK_MIN = 240

// The full set needs this much room: all its columns and the task column at its minimum.
const FULL_FROM = 1160

export const COLUMN_LABEL: Record<ColumnKey, string> = {
  id: 'Id',
  task: 'Task',
  state: 'State',
  agent: 'Agent',
  branch: 'Branch',
  changes: 'Changes',
  pr: 'PR',
  cost: 'Cost',
  activity: 'Last activity',
}

type HasValue = { changes: boolean; pr: boolean; cost: boolean }

// Which columns the table has at a given width of the main area (section 4.25 of the UI spec).
// Changes, PR and Cost are left out when no row has a value. Null width: not measured, so the
// widest set.
export function tableLayout(mainWidth: number | null, has: HasValue): TableLayout {
  const width = mainWidth ?? Number.POSITIVE_INFINITY
  if (width < 560) {
    return { kind: 'cards' }
  }
  let columns: ColumnKey[]
  if (width >= FULL_FROM) {
    columns = ['id', 'task', 'state', 'agent', 'branch', 'changes', 'pr', 'cost', 'activity']
  } else if (width >= 760) {
    columns = ['id', 'task', 'state', 'changes', 'activity']
  } else {
    columns = ['id', 'task', 'state', 'activity']
  }
  columns = columns.filter(
    (column) =>
      !(column === 'changes' && !has.changes) &&
      !(column === 'pr' && !has.pr) &&
      !(column === 'cost' && !has.cost),
  )
  const fixed = columns.reduce((sum, column) => sum + (WIDTH[column] ?? 0), 0)
  return {
    kind: 'table',
    columns,
    widths: Object.fromEntries(columns.map((column) => [column, WIDTH[column]])),
    minWidth: fixed + TASK_MIN,
  }
}
