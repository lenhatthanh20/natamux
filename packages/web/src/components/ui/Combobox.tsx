import { Plus } from 'lucide-react'
import { type KeyboardEvent, useId, useState } from 'react'
import { fieldBox, ICON } from './styles'

export type ComboboxOption = {
  value: string
  label: string
  // Shown first, in mono: for example a task id.
  hint?: string
}

type ComboboxProps = {
  label: string
  placeholder: string
  options: readonly ComboboxOption[]
  // Called with the value that was picked. The field empties itself.
  onPick: (value: string) => void
  className?: string
}

export function matchOptions(options: readonly ComboboxOption[], query: string): ComboboxOption[] {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean)
  return options.filter((option) => {
    const text = `${option.hint ?? ''} ${option.label}`.toLowerCase()
    return words.every((word) => text.includes(word))
  })
}

// A field that filters a list as you type. ARIA combobox with a listbox: Up and Down move,
// Enter picks, Escape closes.
export function Combobox({ label, placeholder, options, onPick, className = '' }: ComboboxProps) {
  const [query, setQuery] = useState('')
  const [open, setOpen] = useState(false)
  const [active, setActive] = useState(0)
  const listId = useId()
  const matches = matchOptions(options, query)
  const current = Math.min(active, Math.max(matches.length - 1, 0))

  const pick = (option: ComboboxOption | undefined) => {
    if (option === undefined) {
      return
    }
    setQuery('')
    setOpen(false)
    onPick(option.value)
  }

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      event.preventDefault()
      setOpen(true)
      const step = event.key === 'ArrowDown' ? 1 : -1
      setActive((current + step + matches.length) % Math.max(matches.length, 1))
    } else if (event.key === 'Enter' && open) {
      event.preventDefault()
      pick(matches[current])
    } else if (event.key === 'Escape' && open) {
      event.preventDefault()
      event.stopPropagation()
      setOpen(false)
    }
  }

  return (
    <div className={`relative ${className}`}>
      <div className={`${fieldBox} h-(--control-h)`}>
        <Plus size={ICON.sm} aria-hidden="true" className="shrink-0" />
        <input
          type="text"
          role="combobox"
          aria-label={label}
          aria-expanded={open}
          aria-controls={listId}
          aria-autocomplete="list"
          aria-activedescendant={open && matches.length > 0 ? `${listId}-${current}` : undefined}
          value={query}
          placeholder={placeholder}
          autoComplete="off"
          onChange={(event) => {
            setQuery(event.target.value)
            setActive(0)
            setOpen(true)
          }}
          onFocus={() => setOpen(true)}
          onBlur={() => setOpen(false)}
          onKeyDown={onKeyDown}
          className="min-w-0 flex-1 bg-transparent text-ink outline-none placeholder:text-placeholder"
        />
      </div>
      {open && (
        <ul
          id={listId}
          role="listbox"
          aria-label={label}
          className="hm-pop absolute top-full right-0 left-0 z-(--z-popover) mt-1 max-h-72 overflow-y-auto rounded-lg border border-line-strong bg-popover p-1 shadow-pop ring-1 ring-contrast"
        >
          {matches.length === 0 && <li className="px-2 py-1.5 text-muted">No match</li>}
          {matches.map((option, index) => (
            <li
              key={option.value}
              id={`${listId}-${index}`}
              role="option"
              aria-selected={index === current}
              // A press picks before the field blurs and closes the list.
              onPointerDown={(event) => {
                event.preventDefault()
                pick(option)
              }}
              onPointerEnter={() => setActive(index)}
              className="flex h-(--row-h) cursor-pointer items-center gap-2 rounded px-2 text-ink-2 aria-selected:bg-hover aria-selected:text-ink"
            >
              {option.hint !== undefined && (
                <span className="shrink-0 font-mono text-xs text-muted">{option.hint}</span>
              )}
              <span className="min-w-0 truncate">{option.label}</span>
            </li>
          ))}
        </ul>
      )}
    </div>
  )
}
