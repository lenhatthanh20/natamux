import { CircleQuestionMark } from 'lucide-react'
import type { AgentId } from '../lib/agents'
import { bannerText, type WaitReason } from '../lib/workspace'
import { StartAgentControl } from './StartAgentControl'
import { Button } from './ui/Button'
import { ICON } from './ui/styles'

type NeedsInputBannerProps = {
  reason: WaitReason
  resuming: boolean
  onAnswer: () => void
  onResume: () => void
  // For a failed task: start an agent again, in place of Resume.
  onStartAgent?: (agent: AgentId) => void
  agent?: string | null
}

// Under the workspace header while the agent waits: the question as text, and what to do.
export function NeedsInputBanner({
  reason,
  resuming,
  onAnswer,
  onResume,
  onStartAgent,
  agent = null,
}: NeedsInputBannerProps) {
  return (
    <div
      role="status"
      className="flex min-h-10 shrink-0 flex-wrap items-center gap-x-3 gap-y-1 border-y border-signal-line bg-signal-soft px-4 py-1"
    >
      <CircleQuestionMark size={ICON.md} aria-hidden="true" className="shrink-0 text-signal" />
      <p className="line-clamp-2 min-w-0 flex-1 basis-60 text-ink" data-tip-overflow>
        {bannerText(reason)}
      </p>
      <div className="flex items-center gap-2">
        {reason.kind === 'asked' && <Button onClick={onAnswer}>Answer in terminal</Button>}
        {onStartAgent === undefined ? (
          <Button variant="ghost" busy={resuming} onClick={onResume}>
            Resume
          </Button>
        ) : (
          <StartAgentControl current={agent} onStart={onStartAgent} />
        )}
      </div>
    </div>
  )
}
