import type { LucideIcon } from 'lucide-react'
import { type KeyboardEvent, type ReactNode, useRef } from 'react'
import { ICON } from './styles'

export type TabItem<T extends string> = {
  id: T
  label: string
  icon?: LucideIcon
  count?: number
}

type TabsProps<T extends string> = {
  label: string
  // Ties each tab to its panel with aria-controls.
  idPrefix: string
  tabs: readonly TabItem<T>[]
  value: T
  onChange: (value: T) => void
  // Actions of the current tab, at the right end of the row.
  actions?: ReactNode
  className?: string
}

export function tabId(prefix: string, id: string): string {
  return `${prefix}-tab-${id}`
}

// The props a panel needs to match its tab.
export function tabPanelProps(prefix: string, id: string, label: string) {
  return {
    role: 'tabpanel' as const,
    id: `${prefix}-panel-${id}`,
    'aria-labelledby': tabId(prefix, id),
    'aria-label': label,
  }
}

// A row of tabs, 36px high. One tab stop; arrows move and open the tab.
export function Tabs<T extends string>({
  label,
  idPrefix,
  tabs,
  value,
  onChange,
  actions,
  className = '',
}: TabsProps<T>) {
  const list = useRef<HTMLDivElement>(null)

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const index = tabs.findIndex((tab) => tab.id === value)
    let next = -1
    if (event.key === 'ArrowRight') {
      next = (index + 1) % tabs.length
    } else if (event.key === 'ArrowLeft') {
      next = (index - 1 + tabs.length) % tabs.length
    } else if (event.key === 'Home') {
      next = 0
    } else if (event.key === 'End') {
      next = tabs.length - 1
    }
    const target = tabs[next]
    if (target === undefined) {
      return
    }
    event.preventDefault()
    onChange(target.id)
    document.getElementById(tabId(idPrefix, target.id))?.focus()
  }

  return (
    <div className={`flex h-9 shrink-0 items-stretch border-b border-line ${className}`}>
      <div
        ref={list}
        role="tablist"
        aria-label={label}
        onKeyDown={onKeyDown}
        className="flex min-w-0 flex-1 items-stretch overflow-x-auto"
      >
        {tabs.map((tab) => {
          const selected = tab.id === value
          const Icon = tab.icon
          return (
            <button
              key={tab.id}
              id={tabId(idPrefix, tab.id)}
              type="button"
              role="tab"
              aria-selected={selected}
              aria-controls={`${idPrefix}-panel-${tab.id}`}
              tabIndex={selected ? 0 : -1}
              onClick={() => onChange(tab.id)}
              className={`focus-inset relative flex shrink-0 items-center gap-1.5 px-2.5 text-sm font-medium ${
                selected ? 'text-ink' : 'text-muted hover:text-ink'
              }`}
            >
              {Icon !== undefined && <Icon size={ICON.sm} aria-hidden="true" />}
              {tab.label}
              {tab.count !== undefined && (
                <span className="inline-flex h-4 min-w-4 items-center justify-center rounded-full bg-badge px-[5px] text-2xs font-semibold text-ink-2">
                  {tab.count}
                </span>
              )}
              {selected && <span className="absolute inset-x-0 bottom-0 h-0.5 bg-ink" />}
            </button>
          )
        })}
      </div>
      {actions !== undefined && <div className="flex items-center gap-1 pr-2">{actions}</div>}
    </div>
  )
}
