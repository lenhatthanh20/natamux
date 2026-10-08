import type { LucideIcon } from 'lucide-react'
import { type KeyboardEvent, useRef } from 'react'
import { ICON } from './styles'

export type SegmentedOption<T extends string> = {
  value: T
  // The name of the option, never the current state.
  label: string
  icon?: LucideIcon
  // Why the option cannot be picked now. It stays focusable-looking but does nothing.
  disabledReason?: string
}

type SegmentedProps<T extends string> = {
  label: string
  options: readonly SegmentedOption<T>[]
  value: T
  onChange: (value: T) => void
  // Icons only, with the names in tooltips: for narrow places.
  iconOnly?: boolean
}

// 2 to 4 exclusive options, like Table and Board. One tab stop; arrows move and pick.
export function Segmented<T extends string>({
  label,
  options,
  value,
  onChange,
  iconOnly = false,
}: SegmentedProps<T>) {
  const group = useRef<HTMLDivElement>(null)

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const enabled = options.filter((option) => option.disabledReason === undefined)
    const index = enabled.findIndex((option) => option.value === value)
    let next = -1
    if (event.key === 'ArrowRight' || event.key === 'ArrowDown') {
      next = (index + 1) % enabled.length
    } else if (event.key === 'ArrowLeft' || event.key === 'ArrowUp') {
      next = (index - 1 + enabled.length) % enabled.length
    } else if (event.key === 'Home') {
      next = 0
    } else if (event.key === 'End') {
      next = enabled.length - 1
    }
    const target = enabled[next]
    if (target === undefined) {
      return
    }
    event.preventDefault()
    onChange(target.value)
    group.current?.querySelector<HTMLElement>(`[data-value="${target.value}"]`)?.focus()
  }

  return (
    // biome-ignore lint/a11y/useSemanticElements: a radiogroup of buttons styled as one control.
    <div
      ref={group}
      role="radiogroup"
      aria-label={label}
      onKeyDown={onKeyDown}
      className="inline-flex h-7 shrink-0 items-center gap-0.5 rounded-md border border-line bg-raised p-0.5"
    >
      {options.map((option) => {
        const selected = option.value === value
        const Icon = option.icon
        const blocked = option.disabledReason !== undefined
        return (
          // biome-ignore lint/a11y/useSemanticElements: see above.
          <button
            key={option.value}
            type="button"
            role="radio"
            aria-checked={selected}
            aria-disabled={blocked ? true : undefined}
            aria-label={iconOnly ? option.label : undefined}
            data-value={option.value}
            data-tip={blocked ? option.disabledReason : iconOnly ? option.label : undefined}
            tabIndex={selected ? 0 : -1}
            onClick={() => {
              if (!blocked) {
                onChange(option.value)
              }
            }}
            className={`flex h-[22px] items-center gap-1.5 rounded px-2 text-xs font-medium aria-disabled:opacity-50 ${
              selected
                ? 'bg-surface text-ink shadow-[0_1px_2px_rgb(0_0_0/0.2)] dark:bg-[#2a2c31]'
                : 'text-muted not-aria-disabled:hover:text-ink'
            }`}
          >
            {Icon !== undefined && <Icon size={ICON.sm} aria-hidden="true" />}
            {!iconOnly && option.label}
          </button>
        )
      })}
    </div>
  )
}
