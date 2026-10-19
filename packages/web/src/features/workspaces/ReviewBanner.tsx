import { stageName, type Task } from '@natamux/core'
import { CircleDot } from 'lucide-react'
import { Banner } from '../../components/Banner'
import { Button } from '../../components/ui/Button'
import { shortcutOf } from '../../lib/commands'
import { reviewStages } from '../../lib/workspace'

type ReviewBannerProps = {
  task: Pick<Task, 'stage' | 'lanes'>
  // The stage whose approval is on its way. Null: none.
  approving: string | null
  onApprove: (stage: string) => void
  onSendBack: () => void
}

// Under the workspace header on a phone while a stage waits for review. Wider screens show it in
// the current stage card of the details.
export function ReviewBanner({ task, approving, onApprove, onSendBack }: ReviewBannerProps) {
  const stages = reviewStages(task)
  const names = stages.map((id) => stageName({ id }))
  const busy = approving !== null
  return (
    <Banner
      tone="review"
      icon={CircleDot}
      actions={
        <>
          {stages.length === 1 ? (
            <Button
              variant="primary"
              busy={busy}
              data-tip="Approve"
              data-tip-kbd={shortcutOf('task.review.approved')}
              onClick={() => onApprove(stages[0] ?? task.stage)}
            >
              Approve
            </Button>
          ) : (
            stages.map((id, index) => (
              <Button
                key={id}
                busy={approving === id}
                disabled={busy && approving !== id}
                onClick={() => onApprove(id)}
              >
                Approve {names[index]}
              </Button>
            ))
          )}
          <Button variant="ghost" disabled={busy} onClick={onSendBack}>
            Send back
          </Button>
        </>
      }
    >
      Ready for review: {names.join(' and ')}
    </Banner>
  )
}
