import type { StageProgress } from '@natamux/core'
import { useState } from 'react'
import { Button } from '../../components/ui/Button'
import { Dialog } from '../../components/ui/Dialog'
import { Select } from '../../components/ui/Select'
import { textarea } from '../../components/ui/styles'

export function SendBackDialog({
  stage,
  earlier,
  busy,
  onClose,
  onSend,
}: {
  stage: StageProgress
  earlier: readonly StageProgress[]
  busy: boolean
  onClose: () => void
  onSend: (to: string, reason: string) => void
}) {
  const [to, setTo] = useState(earlier.at(-1)?.stage ?? stage.stage)
  const [reason, setReason] = useState('')
  return (
    <Dialog
      title="Send the task back"
      description="The stage runs again, and the agent gets your reason with its prompt."
      size="md"
      busy={busy}
      onClose={onClose}
      onSubmit={() => onSend(to, reason.trim())}
      footer={
        <>
          <Button variant="ghost" size="md" onClick={onClose}>
            Cancel
          </Button>
          <Button
            variant="primary"
            size="md"
            busy={busy}
            shortcut="Mod+Enter"
            onClick={() => onSend(to, reason.trim())}
          >
            Send back
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-3">
        <div className="flex flex-col gap-1">
          <span className="text-xs font-medium text-muted">Back to</span>
          <Select
            value={to}
            onChange={(event) => setTo(event.target.value)}
            aria-label="Stage to go back to"
            fieldSize="md"
            className="w-full"
          >
            {earlier.map((entry) => (
              <option key={entry.stage} value={entry.stage}>
                {entry.name}
              </option>
            ))}
          </Select>
        </div>
        <label className="flex flex-col gap-1">
          <span className="text-xs font-medium text-muted">What should change?</span>
          <textarea
            value={reason}
            onChange={(event) => setReason(event.target.value)}
            rows={4}
            data-autofocus
            className={`${textarea} resize-y`}
            placeholder="Add the error cases for an empty list."
          />
        </label>
      </div>
    </Dialog>
  )
}
