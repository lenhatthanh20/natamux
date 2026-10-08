import { useQuery } from '@tanstack/react-query'
import type { StoredEvent } from '../lib/api'
import { describeEvent, formatClock } from '../lib/events'

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
    <section aria-label="Activity" className="flex flex-col gap-2">
      <h3 className="font-medium">Activity</h3>
      {query.isPending && <p className="text-muted">Loading activity.</p>}
      {query.isError && <p role="alert">{query.error.message}</p>}
      {lines?.length === 0 && <p className="text-muted">No activity yet.</p>}
      {lines && lines.length > 0 && (
        <ol className="flex flex-col border-l border-line">
          {[...lines].reverse().map(({ seq, accepted, event }) => (
            <li
              key={seq}
              data-accepted={accepted}
              className="relative flex items-baseline justify-between gap-3 py-1 pl-4 data-[accepted=false]:text-muted"
            >
              <span
                aria-hidden="true"
                className="absolute top-[0.8rem] left-[-0.2rem] h-[0.4rem] w-[0.4rem] rounded-full bg-muted"
              />
              <span className="min-w-0">
                {describeEvent(event.type)}
                {!accepted && <span className="text-muted"> (did not fit, ignored)</span>}
                <span className="text-muted"> from {event.source}</span>
              </span>
              <time dateTime={event.at} className="shrink-0 text-muted">
                {formatClock(event.at)}
              </time>
            </li>
          ))}
        </ol>
      )}
    </section>
  )
}
