import { useQuery } from '@tanstack/react-query'
import type { StoredEvent } from '../lib/api'
import { describeEvent, groupEvents } from '../lib/events'
import { formatExact } from '../lib/tasks'
import { Age } from './Age'
import { DetailSection } from './DetailSection'

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
  const lines = query.data
    ? groupEvents(query.data.filter(({ event }) => event.type !== 'agent.active'))
    : undefined

  return (
    <DetailSection id="activity" title="Activity">
      {query.isPending && <p className="text-muted">Loading activity.</p>}
      {query.isError && <p role="alert">{query.error.message}</p>}
      {lines?.length === 0 && <p className="text-muted">No activity yet.</p>}
      {lines && lines.length > 0 && (
        <ol className="ml-1 flex flex-col border-l border-line">
          {[...lines].reverse().map((line) => (
            <li
              key={line.seq}
              data-accepted={line.accepted}
              className="relative flex items-baseline justify-between gap-3 py-1 pl-4 data-[accepted=false]:text-muted"
            >
              <span
                aria-hidden="true"
                className="absolute top-[0.7rem] left-[-0.1875rem] size-[0.3125rem] rounded-full bg-line-strong"
              />
              <span className="min-w-0">
                {describeEvent(line.type)}
                {line.count > 1 && <span className="text-muted"> ×{line.count}</span>}
                {line.detail !== undefined && (
                  <span className="break-words text-muted">: {line.detail}</span>
                )}
                {!line.accepted && <span className="text-muted"> (did not fit, ignored)</span>}
                <span className="text-muted"> · {line.actor}</span>
              </span>
              <time
                dateTime={line.at}
                data-tip={formatExact(line.at)}
                className="shrink-0 text-xs text-muted"
              >
                <Age at={line.at} style="long" />
              </time>
            </li>
          ))}
        </ol>
      )}
    </DetailSection>
  )
}
