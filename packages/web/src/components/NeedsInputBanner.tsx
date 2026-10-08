import { CircleQuestionMark } from 'lucide-react'
import { bannerText } from '../lib/workspace'
import { Button } from './ui/Button'
import { ICON } from './ui/styles'

type NeedsInputBannerProps = {
  // What the agent asked, or null when the daemon has no text for it.
  question: string | null
  resuming: boolean
  onAnswer: () => void
  onResume: () => void
}

// Under the workspace header while the agent waits: the question as text, and what to do.
export function NeedsInputBanner({
  question,
  resuming,
  onAnswer,
  onResume,
}: NeedsInputBannerProps) {
  return (
    <div
      role="status"
      className="flex min-h-10 shrink-0 flex-wrap items-center gap-x-3 gap-y-1 border-y border-signal-line bg-signal-soft px-4 py-1"
    >
      <CircleQuestionMark size={ICON.md} aria-hidden="true" className="shrink-0 text-signal" />
      <p className="min-w-0 flex-1 basis-60 truncate text-ink" data-tip-overflow>
        {bannerText(question)}
      </p>
      <div className="flex items-center gap-2">
        <Button onClick={onAnswer}>Answer in terminal</Button>
        <Button variant="ghost" busy={resuming} onClick={onResume}>
          Resume
        </Button>
      </div>
    </div>
  )
}
