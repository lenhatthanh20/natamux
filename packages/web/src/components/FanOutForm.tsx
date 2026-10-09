import { useState } from 'react'
import { Button } from './ui/Button'
import { Dialog } from './ui/Dialog'
import { Select } from './ui/Select'

type FanOutDialogProps = {
  busy: boolean
  onFanOut: (count: number, start: boolean) => void
  onClose: () => void
}

// More agents on the same task: copies of it, each in its own workspace.
export function FanOutDialog({ busy, onFanOut, onClose }: FanOutDialogProps) {
  const [count, setCount] = useState(2)
  const [start, setStart] = useState(true)

  const submit = () => {
    onClose()
    onFanOut(count, start)
  }

  return (
    <Dialog
      title="Run more tries"
      description="Each try is a copy of this task in its own workspace, so the agents do not meet."
      onClose={onClose}
      onSubmit={submit}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            Cancel
          </Button>
          <Button variant="primary" disabled={busy} shortcut="Mod+Enter" onClick={submit}>
            Run tries
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-4">
        <div className="flex items-center gap-2">
          <span>Run</span>
          <Select
            value={count}
            onChange={(event) => setCount(Number(event.target.value))}
            aria-label="Number of tries"
            fieldSize="md"
            className="w-16"
          >
            {[1, 2, 3, 4, 5].map((value) => (
              <option key={value} value={value}>
                {value}
              </option>
            ))}
          </Select>
          <span>more {count === 1 ? 'try' : 'tries'}</span>
        </div>
        <label className="flex h-7 items-center gap-2">
          <input
            type="checkbox"
            checked={start}
            onChange={(event) => setStart(event.target.checked)}
            className="size-3.5 accent-accent"
          />
          Start Claude Code in each
        </label>
      </div>
    </Dialog>
  )
}
