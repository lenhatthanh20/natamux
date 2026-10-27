import { type DiffStat, type EventType, type Task, type TaskUsage, waitsFor } from '@natamux/core'
import {
  ArrowDown,
  ArrowUp,
  Ban,
  Check,
  CirclePause,
  CirclePlay,
  Copy,
  ExternalLink,
  FileDiff,
  ListPlus,
  type LucideIcon,
  PanelRight,
  Play,
  RotateCcw,
  Trash2,
  Undo2,
} from 'lucide-react'
import { type MouseEvent, memo, useMemo, useRef, useState } from 'react'
import { AgentName } from '../../components/AgentBadge'
import { DiffSize } from '../../components/DiffSize'
import { StateBadge } from '../../components/StateBadge'
import { StatusGlyph } from '../../components/StatusGlyph'
import { ConfirmDialog } from '../../components/ui/Dialog'
import { type MenuItem, openContextMenu } from '../../components/ui/Menu'
import { ICON } from '../../components/ui/styles'
import { useNow } from '../../hooks/useNow'
import { findRoving, rovingItem, useRovingFocus } from '../../hooks/useRovingFocus'
import { useStableCallback } from '../../hooks/useStableCallback'
import { copyText } from '../../lib/clipboard'
import { shortcutOf } from '../../lib/commands'
import { CHECKS_COLOR, pullRequestNumber, pullRequestSummary } from '../../lib/pull-request'
import {
  actionsFor,
  agentAction,
  cancelTaskDescription,
  DELETE_TASK_DESCRIPTION,
  formatAge,
  formatExact,
  isQuiet,
  laneSummary,
  rowLabel,
  type SortKey,
  stageLabel,
  stageStateLabel,
  type TaskAction,
  type TaskSort,
} from '../../lib/tasks'
import { shortBranch } from '../../lib/workspace-groups'
import { TaskChips } from './TaskChips'
import { COLUMN_LABEL, type ColumnKey, tableLayout } from './table-columns'
import { formatTokens, formatUsd } from './usage-format'

type TaskTableProps = {
  // In the order to show. The parent sorts them and keeps them still under the pointer.
  tasks: readonly Task[]
  // The width of the main area, which picks the columns. Null: not measured, show all.
  mainWidth: number | null
  selectedId: string | null
  diffStats?: Readonly<Record<string, DiffStat>>
  // The tokens and estimated cost of each task that has usage.
  usage?: Readonly<Record<string, TaskUsage>>
  // Every task, to tell whether a task still waits for others. The rows can be filtered.
  allTasks?: readonly Task[]
  // What the agent of each task does now.
  activity?: Readonly<Record<string, string>>
  // Show the repository of each task, which only helps when there are several.
  showRepo?: boolean
  sort: TaskSort
  onSort: (key: SortKey) => void
  // A click or the keyboard picks a row. The details follow when `showDetails` is set.
  onSelect: (id: string, showDetails: boolean) => void
  // Enter or a double click: the workspace of the task.
  onOpen: (id: string) => void
  onToggleDetails: (id: string) => void
  // Opens the dialog for a subtask. Only a task with no parent gets the menu item.
  onAddSubtask?: (task: Task) => void
  // A key moved the selection, so the rows should hold still for a moment.
  onKeyboardMove?: () => void
  // The task actions of the row menu, so the table can triage without opening each task.
  onAction?: (task: Task, action: TaskAction) => void
  onStartAgent?: (task: Task) => void
  onDelete?: (task: Task) => void
}

const RIGHT: readonly ColumnKey[] = ['changes', 'cost', 'activity']

const ACTION_ICON: Partial<Record<EventType, LucideIcon>> = {
  'task.readied': Check,
  'task.blocked': CirclePause,
  'agent.resumed': Play,
  'review.approved': Check,
  'review.changes-requested': Undo2,
  'task.unblocked': CirclePlay,
  'task.retried': RotateCcw,
}

// Where the row menu offers to start an agent. Elsewhere the agent works, or the task is
// waiting for a person or finished.
const STARTS_AGENT = new Set(['draft', 'ready', 'failed'])

function HeaderCell({
  column,
  sort,
  onSort,
}: {
  column: ColumnKey
  sort: TaskSort
  onSort: (key: SortKey) => void
}) {
  const sorted = sort?.key === column ? sort.direction : null
  const Arrow = sorted === 'desc' ? ArrowDown : ArrowUp
  return (
    <th
      scope="col"
      aria-sort={sorted === null ? undefined : sorted === 'asc' ? 'ascending' : 'descending'}
      className="sticky top-0 z-(--z-sticky) h-8 border-b border-line bg-surface p-0 font-medium first:rounded-tl-lg last:rounded-tr-lg"
    >
      <button
        type="button"
        onClick={() => onSort(column)}
        className={`focus-inset flex h-8 w-full items-center gap-1 px-3 whitespace-nowrap hover:text-ink ${RIGHT.includes(column) ? 'justify-end' : ''} ${sorted === null ? '' : 'text-ink'}`}
      >
        {COLUMN_LABEL[column]}
        {sorted !== null && <Arrow size={ICON.xs} aria-hidden="true" />}
      </button>
    </th>
  )
}

// What every row gets. The handlers keep their identity, so a row re-renders only when its task
// or its own values change.
type RowProps = {
  task: Task
  selected: boolean
  // The row holds the tab stop of the list.
  tabStop: boolean
  activate: (id: string) => void
  // clicks: the click count of the event (2 for the second click of a double click).
  onSelect: (id: string, clicks: number) => void
  onOpen: (id: string) => void
  onMenu: (event: MouseEvent, task: Task) => void
}

// A row of the narrow list. Rows off screen are not laid out or painted until they scroll near.
const CardRow = memo(function CardRow({
  task,
  selected,
  tabStop,
  activate,
  onSelect,
  onOpen,
  onMenu,
}: RowProps) {
  const now = useNow()
  const quiet = isQuiet(task, now)
  return (
    // biome-ignore lint/a11y/useKeyWithClickEvents: the button inside takes the keys (roving focus).
    <li
      data-selected={selected}
      onClick={(event) => onSelect(task.id, event.detail)}
      onDoubleClick={() => onOpen(task.id)}
      onContextMenu={(event) => onMenu(event, task)}
      className="h-14 border-b border-line [content-visibility:auto] last:border-b-0 hover:bg-hover data-[selected=true]:bg-selected group-focus-within/table:data-[selected=true]:bg-selection-active"
    >
      <button
        type="button"
        {...rovingItem(task.id, tabStop, activate)}
        aria-label={rowLabel(task, now)}
        aria-current={selected ? 'true' : undefined}
        className="focus-inset flex h-full w-full items-center gap-2 px-3 text-left"
      >
        <StatusGlyph state={task.state} quiet={quiet} tip={laneSummary(task)} />
        <span className="flex min-w-0 flex-1 flex-col">
          <span className="truncate font-medium">{task.title}</span>
          <span className="flex items-center gap-1.5 text-xs text-muted">
            <span className="font-mono">{task.id}</span>
            <span aria-hidden="true">·</span>
            <span>{quiet ? 'Quiet' : stageStateLabel(task)}</span>
            <span aria-hidden="true">·</span>
            <span>{formatAge(task.updatedAt, now)}</span>
          </span>
        </span>
      </button>
    </li>
  )
})

const TableRow = memo(function TableRow({
  task,
  columns,
  selected,
  tabStop,
  activate,
  stat,
  usage,
  note,
  waits,
  showRepo,
  onSelect,
  onOpen,
  onMenu,
}: RowProps & {
  columns: readonly ColumnKey[]
  stat: DiffStat | undefined
  usage: TaskUsage | undefined
  // What the agent does now, while it runs.
  note: string | undefined
  waits: string
  showRepo: boolean
}) {
  const now = useNow()
  const cell = (column: ColumnKey) => {
    switch (column) {
      case 'id':
        return <span className="font-mono text-xs text-muted">{task.id}</span>
      case 'task':
        return (
          <div className="flex min-w-0 items-center gap-2">
            <button
              type="button"
              {...rovingItem(task.id, tabStop, activate)}
              aria-label={rowLabel(task, now)}
              aria-current={selected ? 'true' : undefined}
              data-tip-overflow
              data-tip={task.title}
              className="min-w-0 truncate text-left font-medium focus-visible:outline-none"
            >
              {task.title}
            </button>
            <TaskChips task={task} waits={waits} showRepo={showRepo} quiet={isQuiet(task, now)} />
          </div>
        )
      case 'state':
        return <StateBadge state={task.state} stage={stageLabel(task)} lanes={laneSummary(task)} />
      case 'agent':
        return note !== undefined ? (
          <span data-tip-overflow data-tip={note} className="block truncate text-xs text-muted">
            {note}
          </span>
        ) : task.agent === null ? (
          <span className="text-muted">-</span>
        ) : (
          <AgentName agent={task.agent} className="text-muted" />
        )
      case 'branch':
        return task.branch === null ? (
          <span className="text-muted">-</span>
        ) : (
          <span
            data-tip-overflow
            data-tip={task.branch}
            className="block truncate font-mono text-xs text-muted"
          >
            {shortBranch(task)}
          </span>
        )
      case 'changes':
        return <DiffSize stat={stat} />
      case 'pr':
        return task.pr === null ? (
          <span className="text-muted">-</span>
        ) : (
          <span className="flex min-w-0 items-center gap-1.5">
            <a
              href={task.pr}
              target="_blank"
              rel="noreferrer"
              onClick={(event) => event.stopPropagation()}
              className="inline-flex shrink-0 items-center gap-1 text-link hover:underline"
            >
              {pullRequestNumber(task.pr)}
              <ExternalLink size={ICON.xs} aria-hidden="true" />
            </a>
            {task.prStatus !== null && (
              <span
                data-tip={pullRequestSummary(task) ?? undefined}
                className={`truncate text-xs ${CHECKS_COLOR[task.prStatus.checks]}`}
              >
                {task.prStatus.checks === 'none' ? task.prStatus.state : pullRequestSummary(task)}
              </span>
            )}
          </span>
        )
      case 'cost':
        return usage === undefined ? (
          <span className="text-muted">-</span>
        ) : (
          <span
            data-tip={`${formatTokens(usage.tokens)} tokens, estimated at API list prices`}
            className={`tabular-nums ${usage.costUsd === null ? 'text-muted' : ''}`}
          >
            {formatUsd(usage.costUsd)}
          </span>
        )
      case 'activity':
        return (
          <span
            data-tip={`Updated ${formatExact(task.updatedAt)}. Created ${formatExact(task.createdAt)}.`}
            className="whitespace-nowrap text-muted"
          >
            {formatAge(task.updatedAt, now)}
          </span>
        )
    }
  }
  return (
    <tr
      data-selected={selected}
      onClick={(event) => onSelect(task.id, event.detail)}
      onDoubleClick={() => onOpen(task.id)}
      onContextMenu={(event) => onMenu(event, task)}
      className="group h-10 cursor-pointer has-focus-visible:outline-2 has-focus-visible:-outline-offset-2 has-focus-visible:outline-focus"
    >
      {columns.map((column) => (
        <td
          key={column}
          className={`h-10 min-w-0 border-b border-line px-3 group-hover:bg-hover group-data-[selected=true]:bg-selected group-last:border-b-0 group-data-[selected=true]:group-focus-within/table:bg-selection-active first:group-last:rounded-bl-lg last:group-last:rounded-br-lg ${RIGHT.includes(column) ? 'text-right' : ''}`}
        >
          {cell(column)}
        </td>
      ))}
    </tr>
  )
})

export function TaskTable({
  tasks,
  mainWidth,
  selectedId,
  diffStats = {},
  usage = {},
  allTasks = tasks,
  activity = {},
  showRepo = false,
  sort,
  onSort,
  onSelect,
  onOpen,
  onToggleDetails,
  onAddSubtask,
  onKeyboardMove,
  onAction,
  onStartAgent,
  onDelete,
}: TaskTableProps) {
  const [cancelling, setCancelling] = useState<Task | null>(null)
  const [deleting, setDeleting] = useState<Task | null>(null)
  const roving = useRovingFocus({
    keys: tasks.map((task) => task.id),
    preferred: selectedId,
    letters: true,
    onMove: (id) => {
      onSelect(id, false)
      onKeyboardMove?.()
    },
    onKey: (event, id) => {
      if (event.key === 'Enter') {
        onOpen(id)
        return true
      }
      if (event.key === 'i') {
        onToggleDetails(id)
        return true
      }
      if ((event.key === 'F10' && event.shiftKey) || event.key === 'ContextMenu') {
        const row =
          findRoving(event.currentTarget, id)?.closest<HTMLElement>('[role="row"], li') ?? null
        const task = tasks.find((candidate) => candidate.id === id)
        if (row !== null && task !== undefined) {
          openContextMenu({ preventDefault: () => undefined, currentTarget: row }, menuFor(task))
          return true
        }
      }
      return false
    },
  })

  // The same actions as the details, in the same words.
  const taskActionsFor = (task: Task): MenuItem[] => {
    const actions = onAction === undefined ? [] : actionsFor(task.state)
    const items: MenuItem[] = actions
      .filter((action) => !action.confirm)
      .map((action) => ({
        label: action.label,
        icon: ACTION_ICON[action.type],
        onSelect: () => onAction?.(task, action),
      }))
    const agent = agentAction(task)
    if (onStartAgent !== undefined && agent !== null && STARTS_AGENT.has(task.state)) {
      const waits = waitsFor(task, allTasks)
      items.push({
        label: agent === 'restart' ? 'Restart agent' : 'Start agent',
        icon: agent === 'restart' ? RotateCcw : Play,
        disabled: waits.length > 0,
        hint: waits.length > 0 ? `Waits for ${waits.join(', ')} to be done` : undefined,
        onSelect: () => onStartAgent(task),
      })
    }
    return items.length === 0 ? [] : [...items, { type: 'separator' }]
  }

  const menuFor = (task: Task): MenuItem[] => {
    const cancel =
      onAction === undefined ? undefined : actionsFor(task.state).find((action) => action.confirm)
    return [
      {
        label: 'Open workspace',
        icon: FileDiff,
        shortcut: shortcutOf('task.open'),
        onSelect: () => onOpen(task.id),
      },
      {
        label: 'Show details',
        icon: PanelRight,
        shortcut: shortcutOf('task.details'),
        onSelect: () => onSelect(task.id, true),
      },
      ...(task.parentId === null && onAddSubtask !== undefined
        ? [{ label: 'Add subtask…', icon: ListPlus, onSelect: () => onAddSubtask(task) }]
        : []),
      { type: 'separator' },
      ...taskActionsFor(task),
      { label: 'Copy task id', icon: Copy, onSelect: () => copyText(task.id, 'the task id') },
      {
        label: 'Copy branch name',
        icon: Copy,
        disabled: task.branch === null,
        onSelect: () => copyText(task.branch ?? '', 'the branch name'),
      },
      ...(cancel === undefined
        ? []
        : [
            { type: 'separator' as const },
            {
              label: cancel.label,
              icon: Ban,
              danger: true,
              onSelect: () => setCancelling(task),
            },
          ]),
      ...(onDelete === undefined
        ? []
        : [
            ...(cancel === undefined ? [{ type: 'separator' as const }] : []),
            {
              label: 'Delete task',
              icon: Trash2,
              danger: true,
              onSelect: () => setDeleting(task),
            },
          ]),
    ]
  }

  // Cancel asks first, as it does in the details.
  const cancelAction =
    cancelling === null ? undefined : actionsFor(cancelling.state).find((action) => action.confirm)
  const confirmCancel = cancelling !== null && cancelAction !== undefined && (
    <ConfirmDialog
      title={`Cancel ${cancelling.id}?`}
      description={cancelTaskDescription(cancelling)}
      confirmLabel="Cancel task"
      cancelLabel="Keep working"
      onConfirm={() => onAction?.(cancelling, cancelAction)}
      onClose={() => setCancelling(null)}
    />
  )
  const confirmDelete = deleting !== null && onDelete !== undefined && (
    <ConfirmDialog
      title={`Delete ${deleting.id}?`}
      description={DELETE_TASK_DESCRIPTION}
      confirmLabel="Delete task"
      onConfirm={() => onDelete(deleting)}
      onClose={() => setDeleting(null)}
    />
  )

  const hasChanges = tasks.some((task) => diffStats[task.id]?.files)
  const hasPr = tasks.some((task) => task.pr !== null)
  const hasCost = tasks.some((task) => usage[task.id] !== undefined)
  const layout = useMemo(
    () => tableLayout(mainWidth, { changes: hasChanges, pr: hasPr, cost: hasCost }),
    [mainWidth, hasChanges, hasPr, hasCost],
  )

  // The first click can open the details panel, and the page moves under the pointer, so the
  // second click of a double click may land on another row. Both go to the row of the first.
  const firstClick = useRef<string | null>(null)
  const select = useStableCallback((id: string, clicks: number) => {
    if (clicks > 1) {
      return
    }
    firstClick.current = id
    onSelect(id, true)
  })
  const open = useStableCallback((id: string) => {
    onOpen(firstClick.current ?? id)
    firstClick.current = null
  })
  const menu = useStableCallback((event: MouseEvent, task: Task) =>
    openContextMenu(event, menuFor(task), `Actions for ${task.id}`),
  )
  const rowProps = (task: Task) => ({
    task,
    selected: task.id === selectedId,
    tabStop: task.id === roving.stop,
    activate: roving.activate,
    onSelect: select,
    onOpen: open,
    onMenu: menu,
  })

  if (layout.kind === 'cards') {
    return (
      <>
        <ul
          ref={roving.container}
          aria-label="Tasks"
          {...roving.containerProps}
          className="group/table flex flex-col overflow-hidden rounded-lg border border-line bg-surface"
        >
          {tasks.map((task) => (
            <CardRow key={task.id} {...rowProps(task)} />
          ))}
        </ul>
        {confirmCancel}
        {confirmDelete}
      </>
    )
  }

  const { columns, widths, minWidth } = layout
  return (
    <div className="group/table rounded-lg border border-line bg-surface">
      <table
        ref={roving.container}
        aria-label="Tasks"
        {...roving.containerProps}
        style={{ minWidth }}
        className="w-full table-fixed border-separate border-spacing-0 text-left"
      >
        <colgroup>
          {columns.map((column) => (
            <col key={column} style={{ width: widths[column] ?? undefined }} />
          ))}
        </colgroup>
        <thead className="text-xs text-muted">
          <tr>
            {columns.map((column) => (
              <HeaderCell key={column} column={column} sort={sort} onSort={onSort} />
            ))}
          </tr>
        </thead>
        <tbody>
          {tasks.map((task) => (
            <TableRow
              key={task.id}
              {...rowProps(task)}
              columns={columns}
              stat={diffStats[task.id]}
              usage={usage[task.id]}
              note={task.state === 'running' ? activity[task.id] : undefined}
              waits={waitsFor(task, allTasks).join(', ')}
              showRepo={showRepo}
            />
          ))}
        </tbody>
      </table>
      {confirmCancel}
      {confirmDelete}
    </div>
  )
}
