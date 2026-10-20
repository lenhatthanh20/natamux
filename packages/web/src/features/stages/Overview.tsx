import type { StageProgress, Task } from '@natamux/core'
import { useNow } from '../../hooks/useNow'
import { CARD } from './StageCard'
import { formatDuration, stageTime } from './stage-view'

export function Overview({
  task,
  count,
  sentBack,
}: {
  task: Task
  count: { done: number; total: number }
  sentBack: number
}) {
  const now = useNow()
  const finished = task.state === 'done' || task.state === 'cancelled'
  return (
    <section aria-label="Overview" className={`${CARD} px-3 py-2.5`}>
      <dl className="m-0 flex divide-x divide-line">
        <Figure
          label="Stages"
          value={`${count.done}/${count.total}`}
          tip="Stages done out of all"
        />
        <Figure
          label={finished ? 'Start to finish' : 'Total'}
          value={formatDuration(task.createdAt, finished ? task.updatedAt : now)}
          tip={finished ? 'Time from start to finish' : 'Time since the task started'}
        />
        {task.stageSince !== null && !finished && (
          <Figure
            label="This stage"
            value={formatDuration(task.stageSince, now)}
            tip="Time in this stage"
          />
        )}
        {sentBack > 0 && (
          <Figure label="Sent back" value={String(sentBack)} tip="Times the task was sent back" />
        )}
      </dl>
    </section>
  )
}

// Each figure is as wide as its content, so the labels stay on one line at the narrowest panel.
function Figure({ label, value, tip }: { label: string; value: string; tip: string }) {
  return (
    <div className="min-w-0 flex-auto px-3 first:pl-0 last:pr-0" data-tip={tip}>
      <dt className="truncate text-xs text-muted">{label}</dt>
      <dd className="m-0 truncate text-base font-semibold tracking-tight tabular-nums">{value}</dd>
    </div>
  )
}

export function TimeBars({ progress }: { progress: readonly StageProgress[] }) {
  const now = useNow()
  const rows = progress
    .filter((entry) => entry.startedAt !== null)
    .map((entry) => {
      const end =
        entry.status === 'current'
          ? now.getTime()
          : Date.parse(entry.endedAt ?? entry.startedAt ?? '')
      return { entry, minutes: (end - Date.parse(entry.startedAt ?? '')) / 60_000 }
    })
  if (rows.length === 0) {
    return <p className="m-0 text-sm text-muted">No stage has started yet.</p>
  }
  const longest = Math.max(...rows.map((row) => row.minutes), 1)
  return (
    <div className="flex flex-col gap-2">
      {rows.map(({ entry, minutes }) => (
        <div
          key={entry.stage}
          className="grid grid-cols-[minmax(64px,30%)_minmax(0,1fr)_52px] items-center gap-2 text-xs"
        >
          <span className="truncate text-ink-2" data-tip={entry.name}>
            {entry.name}
          </span>
          <span className="h-1.5 overflow-hidden rounded-full bg-raised">
            <span
              data-state={entry.status === 'current' ? (entry.state ?? 'running') : 'done'}
              className="block h-full rounded-full"
              style={{
                width: `${Math.max(4, (minutes / longest) * 100)}%`,
                background: 'var(--chip-fg)',
              }}
            />
          </span>
          <span className="text-right text-muted tabular-nums">
            {stageTime(entry, now)?.replace(' so far', '') ?? ''}
          </span>
        </div>
      ))}
    </div>
  )
}
