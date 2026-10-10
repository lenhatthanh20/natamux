import type { Task } from '@natamux/core'
import { BellRing } from 'lucide-react'
import { useState } from 'react'
import { formatAge, isQuiet, needsMeReason, rowLabel, stageLabel } from '../lib/tasks'
import { StatusGlyph } from './StatusGlyph'
import { CountBadge } from './ui/CountBadge'
import { ICON, sectionTitle } from './ui/styles'

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

// The section title in amber, the focus point of the frame.
const signalTitle = 'text-2xs font-semibold uppercase tracking-[0.06em] text-signal'

// The top of the sidebar: the one place to look to know which task needs you.
export function NeedsMeStrip({ waiting, now, currentId, filtered, onOpen }: NeedsMeStripProps) {
  const [all, setAll] = useState(false)
  const rows = all ? waiting : waiting.slice(0, SHOWN)
  const hidden = waiting.length - SHOWN
  const any = waiting.length > 0

  return (
    <section
      aria-labelledby="needs-me-heading"
      // The empty state keeps a clear border and the same padding, so the text does not move when
      // the frame appears.
      className={`flex flex-col rounded-lg border p-1 ${any ? 'border-signal-line bg-signal-soft' : 'border-transparent'}`}
    >
      <div className="flex h-7 items-center gap-2 px-2">
        {any && <BellRing size={ICON.sm} aria-hidden="true" className="shrink-0 text-signal" />}
        <h2 id="needs-me-heading" className={any ? signalTitle : sectionTitle}>
          Needs you
        </h2>
        {any && (
          <CountBadge count={waiting.length} tone="signal" label={`${waiting.length} waiting`} />
        )}
        {filtered && (
          <span className="ml-auto text-2xs font-semibold tracking-[0.06em] text-muted uppercase">
            Not filtered
          </span>
        )}
      </div>
      {!any && <p className="flex h-7 items-center px-2 text-muted">Nothing needs you</p>}
      {any && (
        <ul className="flex flex-col">
          {rows.map((task) => (
            <li key={task.id}>
              <button
                type="button"
                onClick={() => onOpen(task.id)}
                aria-current={task.id === currentId ? 'true' : undefined}
                aria-label={rowLabel(task, now)}
                className="focus-inset flex w-full flex-col gap-0.5 rounded-md px-2 py-1.5 text-left hover:bg-hover aria-[current=true]:bg-selected"
              >
                {/* The first line is only the title, so a long title has the whole width. */}
                <span className="flex w-full items-center gap-2">
                  <StatusGlyph state={task.state} quiet={isQuiet(task, now)} />
                  <span className="min-w-0 flex-1 truncate font-medium">{task.title}</span>
                </span>
                {/* Indented by the glyph and the gap, so it starts under the title. */}
                <span className="flex w-full items-center gap-2 pl-6 text-xs">
                  {stageLabel(task) !== null && (
                    <span className="min-w-0 truncate text-muted">{stageLabel(task)}</span>
                  )}
                  <span className="shrink-0 text-signal">{needsMeReason(task, now)}</span>
                  <span className="ml-auto shrink-0 text-muted">
                    {formatAge(task.updatedAt, now)}
                  </span>
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
