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
    <section aria-label="Overview" className={`${CARD} p-3`}>
      <div className="grid grid-cols-2 gap-x-4 gap-y-3">
        <Figure value={`${count.done} of ${count.total}`} label="stages done" />
        <Figure
          value={formatDuration(task.createdAt, finished ? task.updatedAt : now)}
          label={finished ? 'from start to finish' : 'since the task started'}
        />
        {task.stageSince !== null && !finished && (
          <Figure value={formatDuration(task.stageSince, now)} label="in this stage" />
        )}
        {sentBack > 0 && (
          <Figure
            value={String(sentBack)}
            label={sentBack === 1 ? 'time sent back' : 'times sent back'}
          />
        )}
      </div>
    </section>
  )
}

function Figure({ value, label }: { value: string; label: string }) {
  return (
    <div className="min-w-0">
      <div className="text-lg font-semibold tracking-tight tabular-nums">{value}</div>
      <div className="text-xs text-muted">{label}</div>
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
