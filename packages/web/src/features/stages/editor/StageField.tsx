import { type ChangeEvent, type ReactNode, useState } from 'react'
import { Button } from '../../../components/ui/Button'

// A text field that writes to the draft on each key. While it has the focus it shows what was
// typed, also when the draft drops the value (empty, or equal to the default).
export function useLocalText(value: string, commit: (text: string) => void) {
  const [local, setLocal] = useState<string | null>(null)
  return {
    value: local ?? value,
    onChange: (event: ChangeEvent<HTMLInputElement | HTMLTextAreaElement>) => {
      setLocal(event.target.value)
      commit(event.target.value)
    },
    onBlur: () => setLocal(null),
  }
}

export function DefaultMark({ isDefault, onReset }: { isDefault: boolean; onReset: () => void }) {
  return isDefault ? (
    <span className="rounded bg-badge px-1.5 text-2xs font-medium text-muted">Default</span>
  ) : (
    <>
      <span className="rounded bg-signal-soft px-1.5 text-2xs font-medium text-signal">
        Changed
      </span>
      <Button variant="ghost" size="xs" onClick={onReset}>
        Use default
      </Button>
    </>
  )
}

export function Field({
  label,
  htmlFor,
  mark,
  help,
  children,
}: {
  label: string
  htmlFor: string
  mark?: ReactNode
  help?: ReactNode
  children: ReactNode
}) {
  const labelId = `${htmlFor}-label`
  return (
    <fieldset aria-labelledby={labelId} className="flex min-w-0 flex-col gap-1.5">
      <div className="flex min-h-6 flex-wrap items-center gap-2">
        <label id={labelId} htmlFor={htmlFor} className="text-xs font-medium text-ink-2">
          {label}
        </label>
        {mark}
      </div>
      {children}
      {help !== undefined && <p className="m-0 text-xs text-muted">{help}</p>}
    </fieldset>
  )
}
