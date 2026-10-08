import type { Task } from '@helmsman/core'
import type { ReactNode } from 'react'
import { formatAge, STATE_LABEL } from '../lib/tasks'

type SidebarTaskRowProps = {
  task: Task
  now: Date
  // One short line under the title: what the agent does, the size of the work, or the wait.
  note?: ReactNode
  current: boolean
  onOpen: (id: string) => void
}

export function SidebarTaskRow({ task, now, note, current, onOpen }: SidebarTaskRowProps) {
  return (
    <li>
      <button
        type="button"
        onClick={() => onOpen(task.id)}
        aria-current={current}
        className="flex w-full flex-col gap-0.5 rounded-md px-2 py-1.5 text-left hover:bg-hover aria-[current=true]:bg-selected"
      >
        <span className="flex w-full items-center gap-2">
          <span className="state-dot" data-state={task.state} aria-hidden="true" />
          <span className="sr-only">{STATE_LABEL[task.state]}</span>
          <span className="min-w-0 flex-1 truncate font-medium">{task.title}</span>
          <span className="shrink-0 text-xs text-muted">
            {formatAge(task.updatedAt, now)}
          </span>
        </span>
        <span className="flex w-full min-w-0 gap-2 pl-4 text-xs text-muted">
          <span className="shrink-0 font-mono">{task.id}</span>
          {note !== undefined && note !== null && <span className="min-w-0 truncate">{note}</span>}
        </span>
      </button>
    </li>
  )
}
