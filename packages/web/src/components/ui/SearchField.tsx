import { ListFilter, X } from 'lucide-react'
import { type KeyboardEvent, type Ref, useState } from 'react'
import { IconButton } from './IconButton'
import { Kbd } from './Kbd'
import { fieldBox, ICON } from './styles'

type SearchFieldProps = {
  value: string
  onChange: (value: string) => void
  placeholder: string
  // The field's name for screen readers.
  label: string
  // A key that focuses the field, shown while it is empty and not focused. For example '/'.
  hint?: string
  inputRef?: Ref<HTMLInputElement>
  className?: string
}

// A filter field. Escape clears it first, and leaves it on the second press.
export function SearchField({
  value,
  onChange,
  placeholder,
  label,
  hint,
  inputRef,
  className = '',
}: SearchFieldProps) {
  const [focused, setFocused] = useState(false)

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === 'Escape' && value !== '') {
      event.preventDefault()
      event.stopPropagation()
      onChange('')
    }
  }

  return (
    <label className={`${fieldBox} h-(--control-h) ${className}`}>
      <ListFilter size={ICON.sm} aria-hidden="true" className="shrink-0" />
      <input
        ref={inputRef}
        value={value}
        onChange={(event) => onChange(event.target.value)}
        onKeyDown={onKeyDown}
        onFocus={() => setFocused(true)}
        onBlur={() => setFocused(false)}
        placeholder={placeholder}
        aria-label={label}
        className="min-w-0 flex-1 bg-transparent text-ink outline-none placeholder:text-placeholder"
      />
      {value !== '' ? (
        <IconButton
          label="Clear the filter"
          icon={X}
          size="xs"
          className="-mr-1.5"
          onClick={() => onChange('')}
        />
      ) : (
        hint !== undefined && !focused && <Kbd shortcut={hint} />
      )}
    </label>
  )
}
