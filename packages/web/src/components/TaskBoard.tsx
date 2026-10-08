import { type DiffStat, TASK_STATES, type Task } from '@helmsman/core'
import { pullRequestNumber } from '../lib/pull-request'
import { formatAge, isQuiet, STATE_LABEL } from '../lib/tasks'

type TaskBoardProps = {
  tasks: readonly Task[]
  now: Date
  selectedId: string | null
  diffStats?: Readonly<Record<string, DiffStat>>
  onSelect: (id: string) => void
}

// The same tasks as the table, in one column per state. Empty columns of finished states are left
// out, so the board stays narrow.
export function TaskBoard({ tasks, now, selectedId, diffStats = {}, onSelect }: TaskBoardProps) {
  const columns = TASK_STATES.map((state) => ({
    state,
    tasks: tasks.filter((task) => task.state === state),
  })).filter(
    (column) =>
      column.tasks.length > 0 || (column.state !== 'done' && column.state !== 'cancelled'),
  )

  return (
    <div className="flex gap-3 overflow-x-auto pb-2">
      {columns.map((column) => (
        <section
          key={column.state}
          aria-label={STATE_LABEL[column.state]}
          className="flex w-64 shrink-0 flex-col gap-2 rounded-md bg-surface/60 p-2"
        >
          <h2 className="flex items-baseline justify-between px-1">
            <span className="state-chip" data-state={column.state}>
              {STATE_LABEL[column.state]}
            </span>
            <span className="text-muted">{column.tasks.length}</span>
          </h2>
          {column.tasks.map((task) => {
            const stat = diffStats[task.id]
            return (
              <button
                key={task.id}
                type="button"
                onClick={() => onSelect(task.id)}
                data-selected={task.id === selectedId}
                className="flex flex-col gap-1 rounded-md border border-line bg-surface p-2 text-left hover:border-accent data-[selected=true]:border-accent data-[selected=true]:ring-1 data-[selected=true]:ring-accent"
              >
                <span className="flex items-baseline gap-2">
                  <span className="font-mono text-muted">{task.id}</span>
                  {isQuiet(task, now) && (
                    <span className="font-medium text-[#8a4b00] dark:text-signal">quiet</span>
                  )}
                </span>
                <span className="font-medium break-words">{task.title}</span>
                <span className="flex flex-wrap gap-x-3 text-muted">
                  {task.agent !== null && <span>{task.agent}</span>}
                  {stat !== undefined && stat.files > 0 && (
                    <span className="font-mono">
                      +{stat.added} −{stat.removed}
                    </span>
                  )}
                  {task.pr !== null && <span>{pullRequestNumber(task.pr)}</span>}
                  <span>{formatAge(task.updatedAt, now)}</span>
                </span>
              </button>
            )
          })}
        </section>
      ))}
    </div>
  )
}
