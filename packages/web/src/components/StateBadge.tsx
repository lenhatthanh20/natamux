import type { TaskState } from '@natamux/core'
import { STATE_LABEL } from '../lib/tasks'

type StateBadgeProps = {
  state: TaskState
  // The stage the task is in. It goes in the tooltip, so the badge keeps its width.
  stage?: string | null
  // In a group: each stage with its own state, such as "Implement: Running", for the tooltip.
  lanes?: string
}

export function StateBadge({ state, stage = null, lanes }: StateBadgeProps) {
  const parts = [stage, STATE_LABEL[state], lanes]
  const tip = parts.filter((part) => typeof part === 'string').join(' · ')
  return (
    <span
      className="state-chip"
      data-state={state}
      {...(stage === null && lanes === undefined ? {} : { 'data-tip': tip })}
    >
      {STATE_LABEL[state]}
    </span>
  )
}
