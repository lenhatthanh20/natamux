import { CircleQuestionMark } from 'lucide-react'
import type { AgentId } from '../lib/agents'
import { bannerText, type WaitReason } from '../lib/workspace'
import { Banner } from './Banner'
import { StartAgentControl } from './StartAgentControl'
import { Button } from './ui/Button'

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
    <Banner
      tone="signal"
      icon={CircleQuestionMark}
      actions={
        <>
          {reason.kind === 'asked' && <Button onClick={onAnswer}>Answer in terminal</Button>}
          {onStartAgent === undefined ? (
            <Button variant="ghost" busy={resuming} onClick={onResume}>
              Resume
            </Button>
          ) : (
            <StartAgentControl current={agent} onStart={onStartAgent} />
          )}
        </>
      }
    >
      {bannerText(reason)}
    </Banner>
  )
}
