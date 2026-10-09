import type { SortKey } from './tasks'

export type ColumnKey = SortKey

export type TableLayout =
  | { kind: 'cards' }
  | { kind: 'table'; columns: ColumnKey[]; widths: Record<string, number | null>; minWidth: number }

// The width of each column in px. Task has none: it takes what is left, and never less than
// TASK_MIN. When the main area is narrower than the sum, the table scrolls sideways.
const WIDTH: Record<ColumnKey, number | null> = {
  id: 72,
  task: null,
  state: 120,
  agent: 120,
  branch: 200,
  changes: 88,
  pr: 120,
  activity: 104,
}

export const TASK_MIN = 240

export const COLUMN_LABEL: Record<ColumnKey, string> = {
  id: 'Id',
  task: 'Task',
  state: 'State',
  agent: 'Agent',
  branch: 'Branch',
  changes: 'Changes',
  pr: 'PR',
  activity: 'Last activity',
}

type HasValue = { changes: boolean; pr: boolean }

// Which columns the table has at a given width of the main area (section 4.25 of the UI spec).
// Changes and PR are left out when no row has a value. Null width: not measured, so the widest set.
export function tableLayout(mainWidth: number | null, has: HasValue): TableLayout {
  const width = mainWidth ?? Number.POSITIVE_INFINITY
  if (width < 560) {
    return { kind: 'cards' }
  }
  let columns: ColumnKey[]
  if (width >= 1000) {
    columns = ['id', 'task', 'state', 'agent', 'branch', 'changes', 'pr', 'activity']
  } else if (width >= 760) {
    columns = ['id', 'task', 'state', 'changes', 'activity']
  } else {
    columns = ['id', 'task', 'state', 'activity']
  }
  columns = columns.filter(
    (column) => !(column === 'changes' && !has.changes) && !(column === 'pr' && !has.pr),
  )
  const fixed = columns.reduce((sum, column) => sum + (WIDTH[column] ?? 0), 0)
  return {
    kind: 'table',
    columns,
    widths: Object.fromEntries(columns.map((column) => [column, WIDTH[column]])),
    minWidth: fixed + TASK_MIN,
  }
}
