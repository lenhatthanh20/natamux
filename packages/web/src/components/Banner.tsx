import type { LucideIcon } from 'lucide-react'
import type { ReactNode } from 'react'
import { ICON } from './ui/styles'

// Amber asks for an answer; the other tones take the colors of their task state.
export type BannerTone = 'signal' | 'failed' | 'review'

const STATE_TONE =
  'border-[color-mix(in_srgb,var(--chip-fg)_30%,transparent)] bg-[color-mix(in_srgb,var(--chip-fg)_8%,transparent)]'

const FRAME: Record<BannerTone, string> = {
  signal: 'border-signal-line bg-signal-soft',
  failed: STATE_TONE,
  review: STATE_TONE,
}

type BannerProps = {
  tone: BannerTone
  icon: LucideIcon
  children: ReactNode
  actions?: ReactNode
}

// A full-width line under the workspace header: what the task waits for, and what to do.
export function Banner({ tone, icon: Icon, children, actions }: BannerProps) {
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
      <p className="line-clamp-2 min-w-0 flex-1 basis-60 text-ink" data-tip-overflow>
        {children}
      </p>
      {actions !== undefined && <div className="flex items-center gap-2">{actions}</div>}
    </div>
  )
}
