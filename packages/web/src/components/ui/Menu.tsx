import { type LucideIcon, MoreHorizontal } from 'lucide-react'
import { type KeyboardEvent, useEffect, useLayoutEffect, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import { iconButton } from './styles'

export type MenuItem = {
  label: string
  icon?: LucideIcon
  onSelect: () => void
  danger?: boolean
  disabled?: boolean
}

type MenuProps = {
  // The trigger's name for screen readers, for example "Actions for T-0004".
  label: string
  items: readonly MenuItem[]
  // Shown only while the row around it is hovered or the menu is open.
  revealOnHover?: boolean
}

const MENU_WIDTH = 208

// A "…" button with a list of actions. The list is drawn over the page, so a scrolling sidebar
// does not cut it off.
export function Menu({ label, items, revealOnHover = false }: MenuProps) {
  const [open, setOpen] = useState(false)
  const [position, setPosition] = useState({ top: 0, left: 0 })
  const trigger = useRef<HTMLButtonElement>(null)
  const list = useRef<HTMLDivElement>(null)

  useLayoutEffect(() => {
    if (!open || !trigger.current) {
      return
    }
    const box = trigger.current.getBoundingClientRect()
    setPosition({
      top: box.bottom + 4,
      left: Math.max(8, Math.min(box.right - MENU_WIDTH, window.innerWidth - MENU_WIDTH - 8)),
    })
    list.current?.querySelector<HTMLElement>('[role="menuitem"]:not([disabled])')?.focus()
  }, [open])

  useEffect(() => {
    if (!open) {
      return
    }
    const onPointerDown = (event: PointerEvent) => {
      const target = event.target as Node
      if (!list.current?.contains(target) && !trigger.current?.contains(target)) {
        setOpen(false)
      }
    }
    const onKeyDown = (event: globalThis.KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.stopPropagation()
        setOpen(false)
        trigger.current?.focus()
      }
    }
    document.addEventListener('pointerdown', onPointerDown)
    window.addEventListener('keydown', onKeyDown, true)
    return () => {
      document.removeEventListener('pointerdown', onPointerDown)
      window.removeEventListener('keydown', onKeyDown, true)
    }
  }, [open])

  const moveFocus = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key !== 'ArrowDown' && event.key !== 'ArrowUp') {
      return
    }
    event.preventDefault()
    const entries = [
      ...(list.current?.querySelectorAll<HTMLElement>('[role="menuitem"]:not([disabled])') ?? []),
    ]
    const index = entries.indexOf(document.activeElement as HTMLElement)
    const step = event.key === 'ArrowDown' ? 1 : -1
    entries[(index + step + entries.length) % entries.length]?.focus()
  }

  return (
    <>
      <button
        ref={trigger}
        type="button"
        aria-label={label}
        title={label}
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={(event) => {
          event.stopPropagation()
          setOpen((value) => !value)
        }}
        className={`${iconButton} size-6 ${revealOnHover && !open ? 'opacity-0 group-hover:opacity-100 focus-visible:opacity-100' : ''}`}
      >
        <MoreHorizontal size={15} aria-hidden="true" />
      </button>
      {open &&
        createPortal(
          <div
            ref={list}
            role="menu"
            aria-label={label}
            tabIndex={-1}
            onKeyDown={moveFocus}
            style={{ top: position.top, left: position.left, width: MENU_WIDTH }}
            className="fixed z-50 flex flex-col rounded-lg border border-line-strong bg-raised p-1 shadow-xl shadow-black/40"
          >
            {items.map((item) => {
              const Icon = item.icon
              return (
                <button
                  key={item.label}
                  type="button"
                  role="menuitem"
                  disabled={item.disabled}
                  data-danger={item.danger === true}
                  onClick={() => {
                    setOpen(false)
                    item.onSelect()
                  }}
                  className="flex h-7 items-center gap-2 rounded-md px-2 text-left hover:bg-hover focus:bg-hover focus:outline-none disabled:opacity-40 data-[danger=true]:text-remove"
                >
                  {Icon !== undefined && (
                    <Icon size={14} aria-hidden="true" className="shrink-0 opacity-80" />
                  )}
                  <span className="truncate">{item.label}</span>
                </button>
              )
            })}
          </div>,
          document.body,
        )}
    </>
  )
}
