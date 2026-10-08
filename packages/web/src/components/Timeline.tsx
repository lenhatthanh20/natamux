import { useQuery } from '@tanstack/react-query'
import type { StoredEvent } from '../lib/api'
import { describeEvent, formatClock } from '../lib/events'
import { sectionTitle } from './ui/styles'

type TimelineProps = {
  taskId: string
  // Changes whenever the task does, so the list is read again.
  updatedAt: string
  loadEvents: () => Promise<StoredEvent[]>
}

export function Timeline({ taskId, updatedAt, loadEvents }: TimelineProps) {
  const query = useQuery({
    queryKey: ['events', taskId, updatedAt],
    queryFn: loadEvents,
  })

  // "Agent is working" lines only keep the time of the last update fresh. They are not news.
  const lines = query.data?.filter(({ event }) => event.type !== 'agent.active')

  return (
    <section aria-label="Activity" className="flex flex-col gap-2 border-t border-line pt-4">
      <h3 className={sectionTitle}>Activity</h3>
      {query.isPending && <p className="text-muted">Loading activity.</p>}
      {query.isError && <p role="alert">{query.error.message}</p>}
      {lines?.length === 0 && <p className="text-muted">No activity yet.</p>}
      {lines && lines.length > 0 && (
        <ol className="ml-1 flex flex-col border-l border-line">
          {[...lines].reverse().map(({ seq, accepted, event }) => (
            <li
              key={seq}
              data-accepted={accepted}
              className="relative flex items-baseline justify-between gap-3 py-1 pl-4 text-[0.8125rem] data-[accepted=false]:text-muted"
            >
              <span
                aria-hidden="true"
                className="absolute top-[0.7rem] left-[-0.1875rem] size-[0.3125rem] rounded-full bg-line-strong"
              />
              <span className="min-w-0">
                {describeEvent(event.type)}
                {!accepted && <span className="text-muted"> (did not fit, ignored)</span>}
                <span className="text-muted"> from {event.source}</span>
              </span>
              <time dateTime={event.at} className="shrink-0 font-mono text-xs text-muted">
                {formatClock(event.at)}
              </time>
            </li>
          ))}
        </ol>
      )}
    </section>
  )
}
