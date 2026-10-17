import type { LucideIcon } from 'lucide-react'
import { type ReactNode, useId, useLayoutEffect, useRef, useState } from 'react'
import { ICON } from './ui/styles'

// Amber asks for an answer; the other tones take the colors of their task state.
export type BannerTone = 'signal' | 'failed' | 'review' | 'done' | 'cancelled'

// A soft wash and line in the color of the state named by a data-state attribute.
export const STATE_TONE =
  'border-[color-mix(in_srgb,var(--chip-fg)_30%,transparent)] bg-[color-mix(in_srgb,var(--chip-fg)_8%,transparent)]'

const FRAME: Record<BannerTone, string> = {
  signal: 'border-signal-line bg-signal-soft',
  failed: STATE_TONE,
  review: STATE_TONE,
  done: STATE_TONE,
  cancelled: STATE_TONE,
}

type BannerProps = {
  tone: BannerTone
  icon: LucideIcon
  children: ReactNode
  actions?: ReactNode
  // One line inside the workspace header, so a state change never changes the size of the shells.
  inline?: boolean
}

// A full-width line under the workspace header: what the task waits for, and what to do.
export function Banner({ tone, icon: Icon, children, actions, inline = false }: BannerProps) {
  const textId = useId()
  const text = useRef<HTMLParagraphElement>(null)
  const [expanded, setExpanded] = useState(false)
  const [clamped, setClamped] = useState(false)

  // A long message is cut to two lines. The end often says how to fix the problem, so More shows
  // the rest, also on a phone, where there is no tooltip.
  // biome-ignore lint/correctness/useExhaustiveDependencies: new text may fit, or no longer fit.
  useLayoutEffect(() => {
    const element = text.current
    if (element === null || expanded) {
      return
    }
    const check = () => setClamped(element.scrollHeight > element.clientHeight + 1)
    check()
    const observer = typeof ResizeObserver === 'undefined' ? null : new ResizeObserver(check)
    observer?.observe(element)
    return () => observer?.disconnect()
  }, [expanded, children])

  if (inline) {
    // No More: it would make the header taller. The tooltip shows a cut text. -my-px keeps the
    // border out of the row height, where buttons fill it (touch screens).
    return (
      <div
        role="status"
        data-state={tone === 'signal' ? undefined : tone}
        className={`-my-px flex min-w-0 flex-1 items-center gap-2 rounded-md border pr-0.5 pl-2.5 ${FRAME[tone]}`}
      >
        <Icon
          size={ICON.md}
          aria-hidden="true"
          className={`shrink-0 ${tone === 'signal' ? 'text-signal' : 'text-(--chip-fg)'}`}
        />
        <p data-tip-overflow className="min-w-0 flex-1 truncate text-ink">
          {children}
        </p>
        {actions !== undefined && <div className="flex shrink-0 items-center gap-1">{actions}</div>}
      </div>
    )
  }

  return (
    <div
      role="status"
      data-state={tone === 'signal' ? undefined : tone}
      className={`flex min-h-10 shrink-0 flex-wrap items-center gap-x-3 gap-y-1 border-y px-4 py-1 ${FRAME[tone]}`}
    >
      <Icon
        size={ICON.md}
        aria-hidden="true"
        className={`shrink-0 ${tone === 'signal' ? 'text-signal' : 'text-(--chip-fg)'}`}
      />
      <div className="flex min-w-0 flex-1 basis-60 items-baseline gap-2">
        <p
          ref={text}
          id={textId}
          className={`min-w-0 flex-1 text-ink ${expanded ? 'break-words' : 'line-clamp-2'}`}
        >
          {children}
        </p>
        {(clamped || expanded) && (
          <button
            type="button"
            aria-expanded={expanded}
            aria-controls={textId}
            onClick={() => setExpanded(!expanded)}
            className="shrink-0 text-xs text-link hover:underline"
          >
            {expanded ? 'Less' : 'More'}
          </button>
        )}
      </div>
      {actions !== undefined && <div className="flex items-center gap-2">{actions}</div>}
    </div>
  )
}
