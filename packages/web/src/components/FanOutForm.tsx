import { type FormEvent, useState } from 'react'
import { Select } from './ui/Select'
import { button } from './ui/styles'

type FanOutFormProps = {
  busy: boolean
  onFanOut: (count: number, start: boolean) => void
}

// More agents on the same task: copies of it, each in its own workspace.
export function FanOutForm({ busy, onFanOut }: FanOutFormProps) {
  const [count, setCount] = useState(2)
  const [start, setStart] = useState(true)

  const submit = (event: FormEvent) => {
    event.preventDefault()
    onFanOut(count, start)
  }

  return (
    <form onSubmit={submit} className="flex flex-col gap-2 rounded-lg border border-line p-3">
      <div className="flex items-center gap-2">
        <span>Try</span>
        <Select
          value={count}
          onChange={(event) => setCount(Number(event.target.value))}
          aria-label="Number of tries"
          className="w-14"
        >
          {[1, 2, 3, 4, 5].map((value) => (
            <option key={value} value={value}>
              {value}
            </option>
          ))}
        </Select>
        <span className="flex-1">more in parallel</span>
        <button type="submit" disabled={busy} className={button()}>
          Fan out
        </button>
      </div>
      <label className="flex items-center gap-2 text-muted">
        <input
          type="checkbox"
          checked={start}
          onChange={(event) => setStart(event.target.checked)}
          className="size-3.5 accent-accent"
        />
        Start Claude Code in each
      </label>
    </form>
  )
}
