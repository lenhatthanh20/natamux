import type { Task } from '@helmsman/core'
import { formatAge, isQuiet, needsMeTasks } from '../lib/tasks'
import { StateBadge } from './StateBadge'

type NeedsMeStripProps = {
  tasks: readonly Task[]
  now: Date
  selectedId: string | null
  onSelect: (id: string) => void
}

export function NeedsMeStrip({ tasks, now, selectedId, onSelect }: NeedsMeStripProps) {
  const waiting = needsMeTasks(tasks, now)
  const headline =
    waiting.length === 0
      ? 'Nothing needs you.'
      : `${waiting.length} ${waiting.length === 1 ? 'task needs' : 'tasks need'} you`

  return (
    <section aria-labelledby="needs-me-heading" className="bg-band text-band-ink">
      <div className="mx-auto max-w-7xl px-4 py-4">
        <h2 id="needs-me-heading" className="text-xl font-semibold">
          {headline}
        </h2>
        {waiting.length > 0 && (
          <ul className="mt-3 divide-y divide-white/10 border-t border-white/10">
            {waiting.map((task) => (
              <li key={task.id}>
                <button
                  type="button"
                  onClick={() => onSelect(task.id)}
                  aria-current={task.id === selectedId}
                  className="flex w-full items-center gap-3 border-l-4 border-signal py-2 pr-2 pl-3 text-left hover:bg-white/5 aria-[current=true]:bg-white/10"
                >
                  <span className="w-20 shrink-0 font-mono text-band-muted">{task.id}</span>
                  <span className="min-w-0 flex-1 truncate text-[0.9375rem] font-medium">
                    {task.title}
                  </span>
                  <StateBadge state={task.state} />
                  <span className="w-28 shrink-0 text-right text-band-muted">
                    {isQuiet(task, now) ? 'quiet' : 'waiting'}{' '}
                    {formatAge(task.updatedAt, now).replace(' ago', '')}
                  </span>
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>
    </section>
  )
}
