import type { LucideIcon } from 'lucide-react'
import { type KeyboardEvent, type ReactNode, useCallback, useEffect, useRef } from 'react'
import { useScrollEdges } from '../../hooks/useScrollEdges'
import { ICON } from './styles'

export type TabItem<T extends string> = {
  id: T
  label: string
  icon?: LucideIcon
  count?: number
  // What the count means, for screen readers: "2 changed files". The number alone without it.
  countLabel?: string
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
  // The color under the row, which the edge fades blend into.
  background?: 'canvas' | 'surface'
}

const FADE = { canvas: 'from-canvas', surface: 'from-surface' } as const

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
  background = 'canvas',
}: TabsProps<T>) {
  const list = useRef<HTMLDivElement | null>(null)
  const [edgesRef, edges] = useScrollEdges<HTMLDivElement>()
  const setList = useCallback(
    (node: HTMLDivElement | null) => {
      list.current = node
      edgesRef(node)
    },
    [edgesRef],
  )

  // A narrow row scrolls: a newly picked tab, also one picked by a key, comes into view.
  const shownValue = useRef(value)
  useEffect(() => {
    if (shownValue.current === value) {
      return
    }
    shownValue.current = value
    const selected = list.current?.querySelector<HTMLElement>('[aria-selected="true"]')
    selected?.scrollIntoView?.({ block: 'nearest', inline: 'nearest' })
  }, [value])

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
      <div className="relative flex min-w-0 flex-1">
        <div
          ref={setList}
          role="tablist"
          aria-label={label}
          onKeyDown={onKeyDown}
          className="flex min-w-0 flex-1 items-stretch overflow-x-auto [scrollbar-width:none]"
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
                // The count is a description, so the tab's name stays its label.
                aria-describedby={
                  tab.count === undefined ? undefined : `${tabId(idPrefix, tab.id)}-count`
                }
                tabIndex={selected ? 0 : -1}
                onClick={() => onChange(tab.id)}
                className={`focus-inset relative flex shrink-0 items-center gap-1.5 px-2.5 text-sm font-medium ${
                  selected ? 'text-ink' : 'text-muted hover:text-ink'
                }`}
              >
                {Icon !== undefined && <Icon size={ICON.sm} aria-hidden="true" />}
                {tab.label}
                {tab.count !== undefined && (
                  <>
                    <span
                      aria-hidden="true"
                      className="inline-flex h-4 min-w-4 items-center justify-center rounded-full bg-badge px-[5px] text-2xs font-semibold text-ink-2"
                    >
                      {tab.count}
                    </span>
                    <span id={`${tabId(idPrefix, tab.id)}-count`} hidden>
                      {tab.countLabel ?? String(tab.count)}
                    </span>
                  </>
                )}
                {selected && <span className="absolute inset-x-0 bottom-0 h-0.5 bg-ink" />}
              </button>
            )
          })}
        </div>
        {/* Fades say there are more tabs to scroll to. */}
        {edges.left && (
          <div
            aria-hidden="true"
            className={`pointer-events-none absolute inset-y-0 left-0 w-4 bg-linear-to-r to-transparent ${FADE[background]}`}
          />
        )}
        {edges.right && (
          <div
            aria-hidden="true"
            className={`pointer-events-none absolute inset-y-0 right-0 w-4 bg-linear-to-l to-transparent ${FADE[background]}`}
          />
        )}
      </div>
      {actions !== undefined && <div className="flex items-center gap-1 pr-2">{actions}</div>}
    </div>
  )
}
