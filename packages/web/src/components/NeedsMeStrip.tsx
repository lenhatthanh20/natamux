import type { Task } from '@helmsman/core'
import { useState } from 'react'
import { formatAge, isQuiet, needsMeReason, rowLabel } from '../lib/tasks'
import { StatusGlyph } from './StatusGlyph'
import { CountBadge } from './ui/CountBadge'
import { sectionTitle } from './ui/styles'

type NeedsMeStripProps = {
  // The tasks that wait for the user, the longest wait first.
  waiting: readonly Task[]
  now: Date
  currentId: string | null
  // A filter is active in the sidebar. This list ignores it, and says so.
  filtered: boolean
  onOpen: (id: string) => void
}

const SHOWN = 3

// The top of the sidebar: the one place to look to know which task needs you.
export function NeedsMeStrip({ waiting, now, currentId, filtered, onOpen }: NeedsMeStripProps) {
  const [all, setAll] = useState(false)
  const rows = all ? waiting : waiting.slice(0, SHOWN)
  const hidden = waiting.length - SHOWN

  return (
    <section
      aria-labelledby="needs-me-heading"
      className={`flex flex-col border-l-2 ${waiting.length > 0 ? 'border-signal-line' : 'border-transparent'}`}
    >
      <div className="flex h-7 items-center gap-2 px-2">
        <h2 id="needs-me-heading" className={sectionTitle}>
          Needs you
        </h2>
        {waiting.length > 0 && (
          <CountBadge count={waiting.length} tone="signal" label={`${waiting.length} waiting`} />
        )}
        {filtered && (
          <span className="ml-auto text-2xs font-semibold tracking-[0.06em] text-muted uppercase">
            Not filtered
          </span>
        )}
      </div>
      {waiting.length === 0 && (
        <p className="flex h-7 items-center px-2 text-muted">Nothing needs you</p>
      )}
      {waiting.length > 0 && (
        <ul className="flex flex-col">
          {rows.map((task) => (
            <li key={task.id}>
              <button
                type="button"
                onClick={() => onOpen(task.id)}
                aria-current={task.id === currentId ? 'true' : undefined}
                aria-label={rowLabel(task, now)}
                className="focus-inset flex h-7 w-full items-center gap-2 rounded-md px-2 text-left hover:bg-hover aria-[current=true]:bg-selected"
              >
                <StatusGlyph state={task.state} quiet={isQuiet(task, now)} />
                <span className="min-w-0 flex-1 truncate font-medium">{task.title}</span>
                <span className="shrink-0 text-xs text-signal">{needsMeReason(task, now)}</span>
                <span className="w-7 shrink-0 text-right text-xs text-muted">
                  {formatAge(task.updatedAt, now)}
                </span>
              </button>
            </li>
          ))}
        </ul>
      )}
      {hidden > 0 && (
        <button
          type="button"
          onClick={() => setAll(!all)}
          aria-expanded={all}
          className="focus-inset flex h-7 items-center rounded-md px-2 text-xs text-link hover:underline"
        >
          {all ? 'Show fewer' : `Show all ${waiting.length}`}
        </button>
      )}
    </section>
  )
}
