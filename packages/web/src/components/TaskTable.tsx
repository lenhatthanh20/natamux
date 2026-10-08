import { type DiffStat, type Task, waitingOn } from '@helmsman/core'
import { CHECKS_COLOR, pullRequestNumber, pullRequestSummary } from '../lib/pull-request'
import { formatAge, isQuiet } from '../lib/tasks'
import { StateBadge } from './StateBadge'

type TaskTableProps = {
  tasks: readonly Task[]
  now: Date
  selectedId: string | null
  diffStats?: Readonly<Record<string, DiffStat>>
  // Every task, to tell whether a task still waits for others. The rows can be filtered.
  allTasks?: readonly Task[]
  // What the agent of each task does now.
  activity?: Readonly<Record<string, string>>
  onSelect: (id: string) => void
}

export function WaitsFor({ ids }: { ids: readonly string[] }) {
  if (ids.length === 0) {
    return null
  }
  return <span className="shrink-0 text-muted">waits for {ids.join(', ')}</span>
}

function DiffSize({ stat }: { stat: DiffStat | undefined }) {
  if (stat === undefined || stat.files === 0) {
    return <span className="text-muted">-</span>
  }
  const files = stat.files === 1 ? '1 file' : `${stat.files} files`
  return (
    <span title={files}>
      <span className="text-add">+{stat.added}</span>{' '}
      <span className="text-remove">−{stat.removed}</span>
    </span>
  )
}

export function TaskTable({
  tasks,
  now,
  selectedId,
  diffStats = {},
  allTasks = tasks,
  activity = {},
  onSelect,
}: TaskTableProps) {
  if (tasks.length === 0) {
    return (
      <p className="rounded-md border border-dashed border-line px-4 py-8 text-center text-muted">
        No tasks match. Add one with the field above.
      </p>
    )
  }

  return (
    <div className="overflow-x-auto rounded-md border border-line bg-surface">
      <table className="w-full min-w-[52rem] border-collapse text-left">
        <thead>
          <tr className="border-b border-line text-muted">
            <th className="px-3 py-2 font-medium">Task</th>
            <th className="px-3 py-2 font-medium">State</th>
            <th className="px-3 py-2 font-medium">Agent</th>
            <th className="px-3 py-2 font-medium">Branch</th>
            <th className="px-3 py-2 text-right font-medium">Changes</th>
            <th className="px-3 py-2 font-medium">Pull request</th>
            <th className="px-3 py-2 text-right font-medium">Age</th>
            <th className="px-3 py-2 text-right font-medium">Last activity</th>
          </tr>
        </thead>
        <tbody>
          {tasks.map((task) => (
            <tr
              key={task.id}
              data-selected={task.id === selectedId}
              className="border-b border-line last:border-b-0 hover:bg-hover data-[selected=true]:bg-selected"
            >
              <td className="max-w-[24rem] px-3 py-2">
                <button
                  type="button"
                  onClick={() => onSelect(task.id)}
                  className="flex w-full min-w-0 flex-wrap items-baseline gap-x-3 text-left"
                >
                  <span className="w-16 shrink-0 font-mono text-muted">{task.id}</span>
                  <span className="truncate font-medium">{task.title}</span>
                  <WaitsFor ids={waitingOn(task, allTasks)} />
                  {task.repo !== null && (
                    <span className="shrink-0 font-mono text-xs text-muted">{task.repo}</span>
                  )}
                  {task.parentId !== null && (
                    <span className="shrink-0 text-muted">part of {task.parentId}</span>
                  )}
                  {task.state === 'running' && activity[task.id] !== undefined && (
                    <span className="w-full truncate pl-[4.75rem] font-mono text-xs text-muted">
                      {activity[task.id]}
                    </span>
                  )}
                </button>
              </td>
              <td className="px-3 py-2">
                <span className="flex items-center gap-2">
                  <StateBadge state={task.state} />
                  {isQuiet(task, now) && <span className="font-medium text-signal">quiet</span>}
                </span>
              </td>
              <td className="px-3 py-2 text-muted">{task.agent ?? '-'}</td>
              <td className="max-w-[14rem] truncate px-3 py-2 font-mono text-muted">
                {task.branch ?? '-'}
              </td>
              <td className="px-3 py-2 text-right whitespace-nowrap font-mono">
                <DiffSize stat={diffStats[task.id]} />
              </td>
              <td className="px-3 py-2 whitespace-nowrap">
                {task.pr === null ? (
                  <span className="text-muted">-</span>
                ) : (
                  <span className="flex items-baseline gap-2">
                    <a href={task.pr} target="_blank" rel="noreferrer" className="text-accent">
                      {pullRequestNumber(task.pr)}
                    </a>
                    {task.prStatus !== null && (
                      <span className={CHECKS_COLOR[task.prStatus.checks]}>
                        {pullRequestSummary(task)}
                      </span>
                    )}
                  </span>
                )}
              </td>
              <td className="px-3 py-2 text-right text-muted" title={task.createdAt}>
                {formatAge(task.createdAt, now).replace(' ago', '')}
              </td>
              <td className="px-3 py-2 text-right text-muted">{formatAge(task.updatedAt, now)}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}
