import { type DiffStat, needsMe, type Task, type TaskState, waitingOn } from '@helmsman/core'
import { ChevronRight, ListTodo } from 'lucide-react'
import { useState } from 'react'
import { usePhone } from '../hooks/useLayoutMode'
import { useScrollEdges } from '../hooks/useScrollEdges'
import { pullRequestNumber } from '../lib/pull-request'
import { formatAge, formatExact, isQuiet, rowLabel, STATE_LABEL, STATE_ORDER } from '../lib/tasks'
import { DiffSize } from './DiffSize'
import { LABEL_CHIP } from './TaskChips'
import { CountBadge } from './ui/CountBadge'
import { EmptyState } from './ui/EmptyState'
import { ICON } from './ui/styles'

type TaskBoardProps = {
  // In the order to show, filtered. The parent sorts them and keeps them still under the pointer.
  tasks: readonly Task[]
  now: Date
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

// A card is tinted on its left edge when it needs the user: that is where to look first.
function Card({
  task,
  now,
  selected,
  stat,
  waits,
  onSelect,
  onOpen,
}: {
  task: Task
  now: Date
  selected: boolean
  stat: DiffStat | undefined
  waits: readonly string[]
  onSelect: (id: string) => void
  onOpen: (id: string) => void
}) {
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
      className={`flex shrink-0 flex-col gap-1.5 rounded-lg border border-line bg-surface px-3 py-2.5 text-left hover:border-line-strong data-[selected=true]:border-accent data-[selected=true]:ring-1 data-[selected=true]:ring-accent ${urgent ? 'border-l-2 border-l-(--chip-fg)' : ''}`}
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
        {task.agent !== null && <span>{task.agent}</span>}
        <DiffSize stat={stat} empty={null} />
        {task.pr !== null && <span>{pullRequestNumber(task.pr)}</span>}
        {waits.length > 0 && <span>waits for {waits.join(', ')}</span>}
      </span>
    </button>
  )
}

// The same tasks as the table, in one column per state, the ones that need the user first. Each column scrolls by itself, so the
// board always fills the height of the view.
export function TaskBoard({
  tasks,
  now,
  selectedId,
  diffStats = {},
  allTasks = tasks,
  stateFilter,
  showFinished,
  onSelect,
  onOpen,
}: TaskBoardProps) {
  const phone = usePhone()
  const [collapsed, setCollapsed] = useState<ReadonlySet<TaskState>>(new Set())
  const [pickedState, setPickedState] = useState<TaskState | null>(null)
  const [scroller, edges] = useScrollEdges<HTMLDivElement>()

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

  const toggle = (state: TaskState) =>
    setCollapsed((set) => {
      const next = new Set(set)
      if (!next.delete(state)) {
        next.add(state)
      }
      return next
    })

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
      <div ref={scroller} className="flex min-h-0 flex-1 gap-3 overflow-x-auto px-4 pb-4">
        {visible.map((column) => {
          const folded = !phone && collapsed.has(column.state)
          return (
            <section
              key={column.state}
              aria-label={STATE_LABEL[column.state]}
              className={`flex shrink-0 flex-col rounded-lg bg-[color-mix(in_srgb,var(--surface)_60%,transparent)] p-2 ${
                phone ? 'w-full' : 'w-68'
              } ${folded ? 'h-auto self-start' : 'min-h-0'}`}
            >
              <h2 className="flex h-9 shrink-0 items-center gap-2 px-1">
                <span className="state-chip" data-state={column.state}>
                  {STATE_LABEL[column.state]}
                </span>
                <CountBadge count={column.tasks.length} />
                {!phone && (
                  <button
                    type="button"
                    aria-expanded={!folded}
                    aria-label={`${folded ? 'Expand' : 'Collapse'} ${STATE_LABEL[column.state]}`}
                    onClick={() => toggle(column.state)}
                    className="ml-auto flex size-6 items-center justify-center rounded-md text-muted hover:bg-hover hover:text-ink"
                  >
                    <ChevronRight
                      size={ICON.xs}
                      aria-hidden="true"
                      className={`transition-transform ${folded ? '' : 'rotate-90'}`}
                    />
                  </button>
                )}
              </h2>
              {!folded && (
                <div className="flex min-h-0 flex-1 flex-col gap-2 overflow-y-auto p-0.5">
                  {column.tasks.length === 0 && (
                    <p className="px-1 text-xs text-muted">Nothing here</p>
                  )}
                  {column.tasks.map((task) => (
                    <Card
                      key={task.id}
                      task={task}
                      now={now}
                      selected={task.id === selectedId}
                      stat={diffStats[task.id]}
                      waits={waitingOn(task, allTasks)}
                      onSelect={onSelect}
                      onOpen={onOpen}
                    />
                  ))}
                </div>
              )}
            </section>
          )
        })}
        {columns.length === 0 && (
          <EmptyState icon={ListTodo} title="No columns to show">
            Pick a state, or turn on Show finished.
          </EmptyState>
        )}
      </div>
      {!phone && edges.left && (
        <div
          aria-hidden="true"
          className="pointer-events-none absolute inset-y-0 left-0 w-4 bg-linear-to-r from-canvas to-transparent"
        />
      )}
      {!phone && edges.right && (
        <div
          aria-hidden="true"
          className="pointer-events-none absolute inset-y-0 right-0 w-4 bg-linear-to-l from-canvas to-transparent"
        />
      )}
    </div>
  )
}
