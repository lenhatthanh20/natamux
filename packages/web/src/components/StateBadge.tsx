import type { TaskState } from '@natamux/core'
import { STATE_LABEL } from '../lib/tasks'

type StateBadgeProps = {
  state: TaskState
  // The stage the task is in. It goes in the tooltip, so the badge keeps its width.
  stage?: string | null
}

export function StateBadge({ state, stage = null }: StateBadgeProps) {
  return (
    <span
      className="state-chip"
      data-state={state}
      {...(stage === null ? {} : { 'data-tip': `${stage} · ${STATE_LABEL[state]}` })}
    >
      {STATE_LABEL[state]}
    </span>
  )
}
