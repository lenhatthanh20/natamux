import type { TaskState } from '@helmsman/core'
import { STATE_LABEL } from '../lib/tasks'

export function StateBadge({ state }: { state: TaskState }) {
  return (
    <span className="state-chip" data-state={state}>
      {STATE_LABEL[state]}
    </span>
  )
}
