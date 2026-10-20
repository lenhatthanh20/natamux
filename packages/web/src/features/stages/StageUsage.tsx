import type { TaskUsage } from '@natamux/core'
import { stageUsage, usageLine } from '../tasks/usage-format'

// "$0.42 est. · 610K tokens" under a stage that used tokens.
export function StageUsage({ usage, stage }: { usage: TaskUsage | undefined; stage: string }) {
  const used = stageUsage(usage, stage)
  return used === undefined ? null : (
    <span className="block text-xs text-muted tabular-nums">
      {usageLine(used.tokens, used.costUsd)}
    </span>
  )
}
