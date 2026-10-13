import { type DiffStat, needsMe, type Task, type TaskState, waitingOn } from '@natamux/core'
import { ChevronLeft, ChevronRight, ListTodo } from 'lucide-react'
import { memo, useState } from 'react'
import { usePhone } from '../hooks/useLayoutMode'
import { useNow } from '../hooks/useNow'
import { useScrollEdges } from '../hooks/useScrollEdges'
import { useStableCallback } from '../hooks/useStableCallback'
import { pullRequestNumber } from '../lib/pull-request'
import {
  formatAge,
  formatExact,
  isQuiet,
  rowLabel,
  STATE_LABEL,
  STATE_ORDER,
  stageLabel,
} from '../lib/tasks'
import { AgentName } from './AgentBadge'
import { DiffSize } from './DiffSize'
import { LABEL_CHIP } from './TaskChips'
import { CountBadge } from './ui/CountBadge'
import { EmptyState } from './ui/EmptyState'
import { IconButton } from './ui/IconButton'
import { ICON } from './ui/styles'

type TaskBoardProps = {
  // In the order to show, filtered. The parent sorts them and keeps them still under the pointer.
  tasks: readonly Task[]
  selectedId: string | null
  diffStats?: Readonly<Record<string, DiffStat>>
  allTasks?: readonly Task[]
  // The state filter. Set: only that column shows.
  stateFilter: TaskState | null
  // Done and cancelled have columns only when asked for.
  showFinished: boolean
  onSelect: (id: string) => void
  onOpen: (id: string) => void
}

const isFinished = (state: TaskState) => state === 'done' || state === 'cancelled'

// A column is 272px and the gap 12px: the arrows scroll by one column.
const COLUMN_STEP = 284
const taskCount = (count: number) => (count === 1 ? '1 task' : `${count} tasks`)
const SCROLL_BUTTON =
  'pointer-events-auto rounded-full border border-line-strong bg-popover text-ink-2 shadow-pop'

// A card is tinted on its left edge when it needs the user: that is where to look first. It
// re-renders only when its own values change, and is not laid out or painted while off screen.
const Card = memo(function Card({
  task,
  selected,
  stat,
  waits,
  onSelect,
  onOpen,
}: {
  task: Task
  selected: boolean
  stat: DiffStat | undefined
  // The ids of the tasks this one still waits for, such as "T-0003, T-0004". Empty: none.
  waits: string
  onSelect: (id: string) => void
  onOpen: (id: string) => void
}) {
  const now = useNow()
  const quiet = isQuiet(task, now)
  const urgent = needsMe(task.state)
  return (
    <button
      type="button"
      data-state={task.state}
      data-selected={selected}
      aria-label={rowLabel(task, now)}
      onClick={() => onSelect(task.id)}
      onDoubleClick={() => onOpen(task.id)}
      className={`flex shrink-0 flex-col gap-1.5 rounded-lg border border-line bg-surface px-3 py-2.5 text-left [contain-intrinsic-height:auto_88px] [content-visibility:auto] hover:border-line-strong data-[selected=true]:border-accent data-[selected=true]:ring-1 data-[selected=true]:ring-accent ${urgent ? 'border-l-2 border-l-(--chip-fg)' : ''}`}
    >
      <span className="flex items-center gap-2">
        <span className="font-mono text-xs text-muted">{task.id}</span>
        {quiet && (
          <span className={`${LABEL_CHIP} border-signal-line bg-signal-soft text-signal`}>
            quiet
          </span>
        )}
        <span data-tip={formatExact(task.updatedAt)} className="ml-auto text-xs text-muted">
          {formatAge(task.updatedAt, now)}
        </span>
      </span>
      <span className="line-clamp-2 font-medium break-words">{task.title}</span>
      <span className="flex flex-wrap items-center gap-x-3 gap-y-0.5 text-xs text-muted empty:hidden">
        {stageLabel(task) !== null && <span>{stageLabel(task)}</span>}
        {task.agent !== null && <AgentName agent={task.agent} />}
        <DiffSize stat={stat} empty={null} />
        {task.pr !== null && <span>{pullRequestNumber(task.pr)}</span>}
        {waits !== '' && <span>waits for {waits}</span>}
      </span>
    </button>
  )
})

// The same tasks as the table, in one column per state, the ones that need the user first. Each column scrolls by itself, so the
// board always fills the height of the view.
export function TaskBoard({
  tasks,
  selectedId,
  diffStats = {},
  allTasks = tasks,
  stateFilter,
  showFinished,
  onSelect,
  onOpen,
}: TaskBoardProps) {
  const phone = usePhone()
  // Columns the user folded, and empty columns the user opened. Other empty columns are folded,
  // so the columns that have tasks fit in view.
  const [collapsed, setCollapsed] = useState<ReadonlySet<TaskState>>(new Set())
  const [expanded, setExpanded] = useState<ReadonlySet<TaskState>>(new Set())
  const [pickedState, setPickedState] = useState<TaskState | null>(null)
  const [scroller, edges] = useScrollEdges<HTMLDivElement>()
  const select = useStableCallback(onSelect)
  const open = useStableCallback(onOpen)

  const shown = STATE_ORDER.filter((state) =>
    stateFilter !== null ? state === stateFilter : showFinished || !isFinished(state),
  )
  const columns = shown.map((state) => ({
    state,
    tasks: tasks.filter((task) => task.state === state),
  }))
  // One column at a time on a phone, picked from a row of states.
  const current =
    columns.find((column) => column.state === pickedState) ??
    columns.find((column) => column.tasks.length > 0) ??
    columns[0]
  const visible = phone ? (current === undefined ? [] : [current]) : columns

  const isFolded = (column: { state: TaskState; tasks: readonly Task[] }) =>
    !phone &&
    (collapsed.has(column.state) ||
      (column.tasks.length === 0 && columns.length > 1 && !expanded.has(column.state)))

  const toggle = (state: TaskState, fold: boolean) => {
    const edit = (add: boolean) => (set: ReadonlySet<TaskState>) => {
      const next = new Set(set)
      if (add) {
        next.add(state)
      } else {
        next.delete(state)
      }
      return next
    }
    setCollapsed(edit(fold))
    setExpanded(edit(!fold))
  }
  const scrollBy = (direction: 1 | -1) => {
    const element = document.getElementById('task-board-columns')
    element?.scrollBy({ left: direction * COLUMN_STEP, behavior: 'smooth' })
  }

  return (
    <div className="relative flex h-full min-h-0 flex-col">
      {phone && (
        <ul aria-label="Show a state" className="flex shrink-0 gap-1 overflow-x-auto px-4 pb-2">
          {columns.map((column) => (
            <li key={column.state}>
              <button
                type="button"
                aria-pressed={current?.state === column.state}
                onClick={() => setPickedState(column.state)}
                className="focus-inset flex h-8 items-center gap-1.5 rounded-md px-2.5 text-xs font-medium whitespace-nowrap text-muted aria-pressed:bg-selected aria-pressed:text-ink"
              >
                <span className="state-dot" data-state={column.state} aria-hidden="true" />
                {STATE_LABEL[column.state]}
                <span className="tabular-nums opacity-70">{column.tasks.length}</span>
              </button>
            </li>
          ))}
        </ul>
      )}
      <div
        ref={scroller}
        id="task-board-columns"
        className="flex min-h-0 flex-1 gap-3 overflow-x-auto px-4 pb-4"
      >
        {visible.map((column) => {
          const folded = isFolded(column)
          if (folded) {
            return (
              <section
                key={column.state}
                aria-label={STATE_LABEL[column.state]}
                className="flex w-10 shrink-0 flex-col rounded-lg bg-[color-mix(in_srgb,var(--surface)_60%,transparent)]"
              >
                <button
                  type="button"
                  aria-expanded={false}
                  aria-label={`Expand ${STATE_LABEL[column.state]}, ${taskCount(column.tasks.length)}`}
                  data-tip={`Expand ${STATE_LABEL[column.state]}`}
                  onClick={() => toggle(column.state, false)}
                  className="focus-inset flex flex-col items-center gap-2 rounded-lg px-1 py-3 text-muted hover:bg-hover hover:text-ink"
                >
                  <span className="state-dot" data-state={column.state} aria-hidden="true" />
                  <CountBadge count={column.tasks.length} />
                  <span className="text-xs font-medium whitespace-nowrap [writing-mode:vertical-rl]">
                    {STATE_LABEL[column.state]}
                  </span>
                </button>
              </section>
            )
          }
          return (
            <section
              key={column.state}
              aria-label={STATE_LABEL[column.state]}
              className={`flex min-h-0 shrink-0 flex-col rounded-lg bg-[color-mix(in_srgb,var(--surface)_60%,transparent)] p-2 ${
                phone ? 'w-full' : 'w-68'
              }`}
            >
              <h2 className="flex h-9 shrink-0 items-center gap-2 px-1">
                <span className="state-chip" data-state={column.state}>
                  {STATE_LABEL[column.state]}
                </span>
                <CountBadge count={column.tasks.length} />
                {!phone && (
                  <button
                    type="button"
                    aria-expanded={true}
                    aria-label={`Collapse ${STATE_LABEL[column.state]}`}
                    onClick={() => toggle(column.state, true)}
                    className="ml-auto flex size-6 items-center justify-center rounded-md text-muted hover:bg-hover hover:text-ink"
                  >
                    <ChevronRight size={ICON.xs} aria-hidden="true" className="rotate-90" />
                  </button>
                )}
              </h2>
              <div className="flex min-h-0 flex-1 flex-col gap-2 overflow-y-auto p-0.5">
                {column.tasks.length === 0 && (
                  <p className="px-1 text-xs text-muted">Nothing here</p>
                )}
                {column.tasks.map((task) => (
                  <Card
                    key={task.id}
                    task={task}
                    selected={task.id === selectedId}
                    stat={diffStats[task.id]}
                    waits={waitingOn(task, allTasks).join(', ')}
                    onSelect={select}
                    onOpen={open}
                  />
                ))}
              </div>
            </section>
          )
        })}
        {columns.length === 0 && (
          <EmptyState icon={ListTodo} title="No columns to show">
            Pick a state, or turn on Show finished.
          </EmptyState>
        )}
      </div>
      {/* The fades lie over the columns, and the arrows say there is more to scroll to. */}
      {!phone && edges.left && (
        <div className="pointer-events-none absolute inset-y-0 left-0 flex w-12 items-center bg-linear-to-r from-canvas to-transparent pl-1">
          <IconButton
            label="Scroll left"
            icon={ChevronLeft}
            onClick={() => scrollBy(-1)}
            className={SCROLL_BUTTON}
          />
        </div>
      )}
      {!phone && edges.right && (
        <div className="pointer-events-none absolute inset-y-0 right-0 flex w-12 items-center justify-end bg-linear-to-l from-canvas to-transparent pr-1">
          <IconButton
            label="Scroll right"
            icon={ChevronRight}
            onClick={() => scrollBy(1)}
            className={SCROLL_BUTTON}
          />
        </div>
      )}
    </div>
  )
}
