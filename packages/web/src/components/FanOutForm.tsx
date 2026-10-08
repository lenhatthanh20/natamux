import { type FormEvent, useState } from 'react'

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
    <form onSubmit={submit} className="flex flex-wrap items-center gap-2">
      <label className="flex items-center gap-1">
        <span>Try</span>
        <select
          value={count}
          onChange={(event) => setCount(Number(event.target.value))}
          aria-label="Number of tries"
          className="rounded-md border border-line bg-canvas px-1 py-0.5"
        >
          {[1, 2, 3, 4, 5].map((value) => (
            <option key={value} value={value}>
              {value}
            </option>
          ))}
        </select>
        <span>more in parallel</span>
      </label>
      <label className="flex items-center gap-1 text-muted">
        <input
          type="checkbox"
          checked={start}
          onChange={(event) => setStart(event.target.checked)}
        />
        start Claude Code in each
      </label>
      <button
        type="submit"
        disabled={busy}
        className="rounded-md border border-line px-2 py-1 font-medium hover:border-accent disabled:opacity-50"
      >
        Fan out
      </button>
    </form>
  )
}
