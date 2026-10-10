import { CircleQuestionMark, CircleX } from 'lucide-react'
import type { AgentId } from '../lib/agents'
import { bannerText, type WaitReason } from '../lib/workspace'
import { Banner } from './Banner'
import { StartAgentControl } from './StartAgentControl'
import { Button } from './ui/Button'

type NeedsInputBannerProps = {
  reason: WaitReason
  // The task failed: red and the failed glyph, as its state chip. Amber is for an answer.
  failed?: boolean
  resuming: boolean
  // An agent start is on its way.
  starting?: boolean
  onAnswer: () => void
  onResume: () => void
  // For a failed task: start an agent again, in place of Resume.
  onStartAgent?: (agent: AgentId) => void
  agent?: string | null
}

// Under the workspace header while the agent waits: the question as text, and what to do.
export function NeedsInputBanner({
  reason,
  failed = false,
  resuming,
  starting = false,
  onAnswer,
  onResume,
  onStartAgent,
  agent = null,
}: NeedsInputBannerProps) {
  return (
    <Banner
      tone={failed ? 'failed' : 'signal'}
      icon={failed ? CircleX : CircleQuestionMark}
      actions={
        <>
          {reason.kind === 'asked' && <Button onClick={onAnswer}>Answer in terminal</Button>}
          {onStartAgent === undefined ? (
            <Button variant="ghost" busy={resuming} onClick={onResume}>
              Resume
            </Button>
          ) : (
            <StartAgentControl current={agent} busy={starting} onStart={onStartAgent} />
          )}
        </>
      }
    >
      {bannerText(reason)}
    </Banner>
  )
}
