import type { SortKey } from './tasks'

export type ColumnKey = SortKey

export type TableLayout =
  | { kind: 'cards' }
  | { kind: 'table'; columns: ColumnKey[]; template: string }

// The width of each column as a grid track. Task takes what is left; Agent and Branch may shrink.
const TRACK: Record<ColumnKey, string> = {
  id: '4.5rem',
  task: 'minmax(10rem, 1fr)',
  state: '7.5rem',
  agent: 'minmax(5.5rem, 7.5rem)',
  branch: 'minmax(7.5rem, 12.5rem)',
  changes: '5.5rem',
  pr: 'minmax(5.5rem, 7.5rem)',
  activity: '6.5rem',
}

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
  return { kind: 'table', columns, template: columns.map((column) => TRACK[column]).join(' ') }
}
