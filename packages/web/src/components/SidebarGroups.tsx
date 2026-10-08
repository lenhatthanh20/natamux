import { type DiffStat, type Task, waitingOn } from '@helmsman/core'
import type { SidebarGroup } from '../lib/sidebar-groups'
import { STATE_LABEL } from '../lib/tasks'
import { SidebarTaskRow } from './SidebarTaskRow'

type SidebarGroupsProps = {
  open: readonly SidebarGroup[]
  finished: readonly Task[]
  now: Date
  currentId: string | null
  allTasks: readonly Task[]
  diffStats?: Readonly<Record<string, DiffStat>>
  activity?: Readonly<Record<string, string>>
  onOpen: (id: string) => void
}

function sizeOf(stat: DiffStat | undefined): string | null {
  return stat === undefined || stat.files === 0 ? null : `+${stat.added} −${stat.removed}`
}

// The open work below the tasks that need the user, then the finished tasks, folded.
export function SidebarGroups({
  open,
  finished,
  now,
  currentId,
  allTasks,
  diffStats = {},
  activity = {},
  onOpen,
}: SidebarGroupsProps) {
  const noteFor = (task: Task) => {
    const waits = waitingOn(task, allTasks)
    if (waits.length > 0) {
      return `waits for ${waits.join(', ')}`
    }
    const size = sizeOf(diffStats[task.id])
    if (task.state === 'running') {
      return activity[task.id] ?? size
    }
    return size
  }

  return (
    <>
      {open.map((group) => (
        <section
          key={group.state}
          aria-labelledby={`sidebar-${group.state}`}
          className="flex flex-col gap-0.5"
        >
          <h3
            id={`sidebar-${group.state}`}
            className="flex justify-between px-3 py-1 text-xs font-semibold text-muted"
          >
            <span>{STATE_LABEL[group.state]}</span>
            <span>{group.tasks.length}</span>
          </h3>
          <ul className="flex flex-col">
            {group.tasks.map((task) => (
              <SidebarTaskRow
                key={task.id}
                task={task}
                now={now}
                note={noteFor(task)}
                current={task.id === currentId}
                onOpen={onOpen}
              />
            ))}
          </ul>
        </section>
      ))}
      {finished.length > 0 && (
        <details>
          <summary className="flex cursor-default justify-between rounded-md px-3 py-1 text-xs font-semibold text-muted hover:bg-hover">
            <span>Done and cancelled</span>
            <span>{finished.length}</span>
          </summary>
          <ul className="flex flex-col opacity-75">
            {finished.map((task) => (
              <SidebarTaskRow
                key={task.id}
                task={task}
                now={now}
                current={task.id === currentId}
                onOpen={onOpen}
              />
            ))}
          </ul>
        </details>
      )}
    </>
  )
}
