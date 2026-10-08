import type { Task } from '@helmsman/core'
import { formatAge, isQuiet, STATE_LABEL } from '../lib/tasks'
import { SidebarTaskRow } from './SidebarTaskRow'

type NeedsMeStripProps = {
  // The tasks that wait for the user, the longest wait first.
  waiting: readonly Task[]
  now: Date
  currentId: string | null
  onOpen: (id: string) => void
}

// The top of the sidebar: the one place to look to know which task needs you.
export function NeedsMeStrip({ waiting, now, currentId, onOpen }: NeedsMeStripProps) {
  const headline =
    waiting.length === 0
      ? 'Nothing needs you.'
      : `${waiting.length} ${waiting.length === 1 ? 'task needs' : 'tasks need'} you`

  return (
    <section
      aria-labelledby="needs-me-heading"
      data-waiting={waiting.length > 0}
      className="group/needs flex flex-col gap-0.5 rounded-lg p-1 data-[waiting=true]:bg-signal-soft data-[waiting=true]:ring-1 data-[waiting=true]:ring-signal/25"
    >
      <h2
        id="needs-me-heading"
        className="px-2 py-1 text-xs font-semibold text-muted group-data-[waiting=true]/needs:text-signal"
      >
        {headline}
      </h2>
      {waiting.length > 0 && (
        <ul className="flex flex-col">
          {waiting.map((task) => {
            const age = formatAge(task.updatedAt, now).replace(' ago', '')
            return (
              <SidebarTaskRow
                key={task.id}
                task={task}
                now={now}
                current={task.id === currentId}
                note={
                  isQuiet(task, now) ? `quiet for ${age}` : `${STATE_LABEL[task.state]}, ${age}`
                }
                onOpen={onOpen}
              />
            )
          })}
        </ul>
      )}
    </section>
  )
}
