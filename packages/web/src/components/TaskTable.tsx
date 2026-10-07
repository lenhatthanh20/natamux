import type { Task } from '@helmsman/core'
import { formatAge } from '../lib/tasks'
import { StateBadge } from './StateBadge'

type TaskTableProps = {
  tasks: readonly Task[]
  now: Date
  selectedId: string | null
  onSelect: (id: string) => void
}

export function TaskTable({ tasks, now, selectedId, onSelect }: TaskTableProps) {
  if (tasks.length === 0) {
    return (
      <p className="rounded-md border border-dashed border-line px-4 py-8 text-center text-muted">
        No tasks match. Add one with the field above.
      </p>
    )
  }

  return (
    <div className="overflow-x-auto rounded-md border border-line bg-surface">
      <table className="w-full min-w-[40rem] border-collapse text-left">
        <thead>
          <tr className="border-b border-line text-muted">
            <th className="px-3 py-2 font-medium">Task</th>
            <th className="px-3 py-2 font-medium">State</th>
            <th className="px-3 py-2 font-medium">Agent</th>
            <th className="px-3 py-2 font-medium">Branch</th>
            <th className="px-3 py-2 text-right font-medium">Last activity</th>
          </tr>
        </thead>
        <tbody>
          {tasks.map((task) => (
            <tr
              key={task.id}
              data-selected={task.id === selectedId}
              className="border-b border-line last:border-b-0 data-[selected=true]:bg-accent/10 data-[selected=true]:shadow-[inset_3px_0_0_var(--accent)]"
            >
              <td className="max-w-[24rem] px-3 py-2">
                <button
                  type="button"
                  onClick={() => onSelect(task.id)}
                  className="flex w-full min-w-0 items-baseline gap-3 text-left"
                >
                  <span className="w-16 shrink-0 font-mono text-muted">{task.id}</span>
                  <span className="truncate font-medium">{task.title}</span>
                </button>
              </td>
              <td className="px-3 py-2">
                <StateBadge state={task.state} />
              </td>
              <td className="px-3 py-2 text-muted">{task.agent ?? '-'}</td>
              <td className="max-w-[14rem] truncate px-3 py-2 font-mono text-muted">
                {task.branch ?? '-'}
              </td>
              <td className="px-3 py-2 text-right text-muted">{formatAge(task.updatedAt, now)}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}
