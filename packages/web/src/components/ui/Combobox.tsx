import { Check, ChevronDown, ChevronRight, Plus } from 'lucide-react'
import { type CSSProperties, type KeyboardEvent, useId, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import { fieldBox, ICON } from './styles'

export type ComboboxOption = {
  value: string
  label: string
  // Shown first, in mono: for example a task id.
  hint?: string
  // The heading it is listed under. Options without one come first.
  group?: string
}

type ComboboxProps = {
  label: string
  placeholder: string
  options: readonly ComboboxOption[]
  // Pick mode: called with the value that was picked, and the field empties itself.
  onPick?: (value: string) => void
  // Value mode: the field holds the chosen value and shows its label.
  value?: string
  onChange?: (value: string) => void
  // Groups that show as one row until the user types or opens them.
  collapsedGroups?: readonly string[]
  disabled?: boolean
  // 28px in toolbars, 32px in dialogs.
  fieldSize?: 'sm' | 'md'
  className?: string
}

export function matchOptions(options: readonly ComboboxOption[], query: string): ComboboxOption[] {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean)
  return options.filter((option) => {
    const text = `${option.hint ?? ''} ${option.label}`.toLowerCase()
    return words.every((word) => text.includes(word))
  })
}

type Entry =
  | { type: 'option'; option: ComboboxOption }
  | { type: 'expand'; group: string; count: number }

// The rows of the list, in groups. A collapsed group is one row, until the user searches.
export function comboboxEntries(
  matches: readonly ComboboxOption[],
  collapsedGroups: readonly string[],
  expanded: readonly string[],
  searching: boolean,
): Entry[] {
  const names = [...new Set(matches.flatMap((option) => (option.group ? [option.group] : [])))]
  const entries: Entry[] = matches
    .filter((option) => !option.group)
    .map((option) => ({ type: 'option', option }))
  for (const group of names) {
    const inGroup = matches.filter((option) => option.group === group)
    if (!searching && collapsedGroups.includes(group) && !expanded.includes(group)) {
      entries.push({ type: 'expand', group, count: inGroup.length })
    } else {
      entries.push(...inGroup.map((option): Entry => ({ type: 'option', option })))
    }
  }
  return entries
}

function popupStyle(field: HTMLElement | null): CSSProperties {
  const rect = field?.getBoundingClientRect()
  return rect === undefined ? {} : { top: rect.bottom + 4, left: rect.left, width: rect.width }
}

// A field that filters a list as you type. ARIA combobox with a listbox: Up and Down move,
// Enter picks, Escape closes.
export function Combobox({
  label,
  placeholder,
  options,
  onPick,
  value,
  onChange,
  collapsedGroups = [],
  disabled = false,
  fieldSize = 'sm',
  className = '',
}: ComboboxProps) {
  const [query, setQuery] = useState('')
  const [typing, setTyping] = useState(false)
  const [open, setOpen] = useState(false)
  const [active, setActive] = useState(0)
  const [expanded, setExpanded] = useState<string[]>([])
  const listId = useId()
  const box = useRef<HTMLDivElement>(null)
  const holdsValue = value !== undefined
  const chosen = holdsValue ? options.find((option) => option.value === value) : undefined
  const searching = query.trim() !== ''
  const matches = matchOptions(options, query)
  const entries = comboboxEntries(matches, collapsedGroups, expanded, searching)
  const current = Math.min(active, Math.max(entries.length - 1, 0))

  const close = () => {
    setOpen(false)
    setTyping(false)
    setQuery('')
  }

  const choose = (entry: Entry | undefined) => {
    if (entry === undefined) {
      return
    }
    if (entry.type === 'expand') {
      setExpanded((names) => [...names, entry.group])
      return
    }
    close()
    if (holdsValue) {
      onChange?.(entry.option.value)
    } else {
      onPick?.(entry.option.value)
    }
  }

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      event.preventDefault()
      setOpen(true)
      const step = event.key === 'ArrowDown' ? 1 : -1
      setActive((current + step + entries.length) % Math.max(entries.length, 1))
    } else if (event.key === 'Enter' && open) {
      event.preventDefault()
      choose(entries[current])
    } else if (event.key === 'Escape' && open) {
      event.preventDefault()
      event.stopPropagation()
      close()
    }
  }

  let lastGroup: string | undefined
  const shownText = typing ? query : holdsValue ? (chosen?.label ?? '') : query

  return (
    <div className={`relative ${className}`}>
      <div
        ref={box}
        className={`${fieldBox} ${fieldSize === 'md' ? 'h-(--control-h-md)' : 'h-(--control-h)'} ${disabled ? 'opacity-50' : ''}`}
      >
        {!holdsValue && <Plus size={ICON.sm} aria-hidden="true" className="shrink-0" />}
        <input
          type="text"
          role="combobox"
          aria-label={label}
          aria-expanded={open}
          aria-controls={listId}
          aria-autocomplete="list"
          aria-activedescendant={open && entries.length > 0 ? `${listId}-${current}` : undefined}
          value={shownText}
          placeholder={placeholder}
          autoComplete="off"
          disabled={disabled}
          onChange={(event) => {
            setQuery(event.target.value)
            setTyping(true)
            setActive(0)
            setOpen(true)
          }}
          onFocus={(event) => {
            setOpen(true)
            if (holdsValue) {
              event.target.select()
            }
          }}
          onBlur={close}
          onKeyDown={onKeyDown}
          className={`min-w-0 flex-1 truncate bg-transparent outline-none placeholder:text-placeholder ${holdsValue && !typing ? 'font-mono text-xs text-ink' : 'text-ink'}`}
        />
        {holdsValue && (
          <ChevronDown size={ICON.sm} aria-hidden="true" className="shrink-0 text-muted" />
        )}
      </div>
      {open &&
        createPortal(
          // Drawn over the page at the field's place, so a dialog's edge does not cut it off.
          <div
            id={listId}
            role="listbox"
            aria-label={label}
            style={popupStyle(box.current)}
            className="hm-pop fixed z-(--z-palette) max-h-72 overflow-y-auto rounded-lg border border-line-strong bg-popover p-1 shadow-pop ring-1 ring-contrast"
          >
            {entries.length === 0 && <p className="px-2 py-1.5 text-muted">No match</p>}
            {entries.map((entry, index) => {
              const group = entry.type === 'option' ? entry.option.group : entry.group
              const heading = group !== undefined && group !== lastGroup ? group : null
              lastGroup = group
              return (
                <div key={entry.type === 'option' ? entry.option.value : `group:${entry.group}`}>
                  {heading !== null && entry.type === 'option' && (
                    <p className="flex h-6 items-center px-2 text-2xs font-semibold tracking-[0.06em] text-muted uppercase">
                      {heading}
                    </p>
                  )}
                  <div
                    id={`${listId}-${index}`}
                    role="option"
                    tabIndex={-1}
                    aria-selected={index === current}
                    // A press picks before the field blurs and closes the list.
                    onPointerDown={(event) => {
                      event.preventDefault()
                      choose(entry)
                    }}
                    onPointerEnter={() => setActive(index)}
                    className="flex h-(--row-h) cursor-pointer items-center gap-2 rounded px-2 text-ink-2 aria-selected:bg-hover aria-selected:text-ink"
                  >
                    {entry.type === 'expand' ? (
                      <>
                        <ChevronRight size={ICON.xs} aria-hidden="true" className="shrink-0" />
                        <span className="min-w-0 truncate">
                          {entry.group}
                          <span className="text-muted"> ({entry.count})</span>
                        </span>
                      </>
                    ) : (
                      <>
                        {entry.option.hint !== undefined && (
                          <span className="shrink-0 font-mono text-xs text-muted">
                            {entry.option.hint}
                          </span>
                        )}
                        <span className="min-w-0 flex-1 truncate">{entry.option.label}</span>
                        {holdsValue && entry.option.value === value && (
                          <Check size={ICON.sm} aria-hidden="true" className="shrink-0" />
                        )}
                      </>
                    )}
                  </div>
                </div>
              )
            })}
          </div>,
          document.body,
        )}
    </div>
  )
}
