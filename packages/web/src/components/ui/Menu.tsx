import { Check, Ellipsis, type LucideIcon } from 'lucide-react'
import {
  type KeyboardEvent as ReactKeyboardEvent,
  type RefObject,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from 'react'
import { createPortal } from 'react-dom'
import { create } from 'zustand'
import { formatShortcut } from '../../lib/keys'
import { IconButton } from './IconButton'
import { ICON, type IconButtonSize } from './styles'

export type MenuAction = {
  type?: 'item'
  label: string
  icon?: LucideIcon
  onSelect: () => void
  danger?: boolean
  disabled?: boolean
  // For example 'Mod+Shift+P'. Shown at the right end of the row.
  shortcut?: string
  // A toggle: a check mark shows when true.
  checked?: boolean
}

export type MenuItem = MenuAction | { type: 'separator' } | { type: 'label'; label: string }

type Anchor = { kind: 'rect'; rect: DOMRect } | { kind: 'point'; x: number; y: number }

const GAP = 4
const EDGE = 8
const ENTRY = '[role="menuitem"]:not([disabled]), [role="menuitemcheckbox"]:not([disabled])'

type PopupProps = {
  label: string
  items: readonly MenuItem[]
  anchor: Anchor
  focus: 'first' | 'last'
  onClose: (restoreFocus: boolean) => void
  // Presses on this element are the opener's business, so the list does not close first.
  ignore?: RefObject<HTMLElement | null>
}

// The list itself. A menu button and a context menu both draw it, over the page, so a scrolling
// sidebar does not cut it off.
function MenuPopup({ label, items, anchor, focus, onClose, ignore }: PopupProps) {
  const list = useRef<HTMLDivElement>(null)
  const typed = useRef({ text: '', at: 0 })
  const [position, setPosition] = useState<{ top: number; left: number; flipped: boolean } | null>(
    null,
  )

  // Place it, flip it above when there is no room below, and focus the first or last item.
  useLayoutEffect(() => {
    const box = list.current
    if (box === null) {
      return
    }
    const size = box.getBoundingClientRect()
    const rect =
      anchor.kind === 'rect'
        ? anchor.rect
        : { top: anchor.y, bottom: anchor.y, left: anchor.x, right: anchor.x }
    const spaceBelow = window.innerHeight - rect.bottom - GAP - EDGE
    const flipped = size.height > spaceBelow && rect.top - GAP - EDGE > spaceBelow
    // A menu button's list lines up with its right edge; a context menu opens at the pointer.
    const wanted = anchor.kind === 'rect' ? rect.right - size.width : rect.left
    setPosition({
      top: flipped ? Math.max(EDGE, rect.top - GAP - size.height) : rect.bottom + GAP,
      left: Math.max(EDGE, Math.min(wanted, window.innerWidth - size.width - EDGE)),
      flipped,
    })
    const entries = box.querySelectorAll<HTMLElement>(ENTRY)
    const target = focus === 'last' ? entries[entries.length - 1] : entries[0]
    target?.focus()
  }, [anchor, focus])

  useEffect(() => {
    const onPointerDown = (event: PointerEvent) => {
      const target = event.target as Node
      if (!list.current?.contains(target) && !ignore?.current?.contains(target)) {
        onClose(false)
      }
    }
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.stopPropagation()
        onClose(true)
      }
    }
    const onBlur = () => onClose(false)
    document.addEventListener('pointerdown', onPointerDown, true)
    window.addEventListener('keydown', onKeyDown, true)
    window.addEventListener('blur', onBlur)
    window.addEventListener('resize', onBlur)
    return () => {
      document.removeEventListener('pointerdown', onPointerDown, true)
      window.removeEventListener('keydown', onKeyDown, true)
      window.removeEventListener('blur', onBlur)
      window.removeEventListener('resize', onBlur)
    }
  }, [onClose, ignore])

  const moveFocus = (event: ReactKeyboardEvent<HTMLDivElement>) => {
    const entries = [...(list.current?.querySelectorAll<HTMLElement>(ENTRY) ?? [])]
    const index = entries.indexOf(document.activeElement as HTMLElement)
    const go = (next: number) => {
      event.preventDefault()
      entries[(next + entries.length) % entries.length]?.focus()
    }
    if (event.key === 'ArrowDown') {
      go(index + 1)
    } else if (event.key === 'ArrowUp') {
      go(index === -1 ? entries.length - 1 : index - 1)
    } else if (event.key === 'Home') {
      go(0)
    } else if (event.key === 'End') {
      go(entries.length - 1)
    } else if (event.key === 'Tab') {
      // Focus goes back to the opener and Tab moves on from there.
      onClose(true)
    } else if (event.key.length === 1 && !event.ctrlKey && !event.metaKey && event.key !== ' ') {
      const now = Date.now()
      const text =
        (now - typed.current.at > 600 ? '' : typed.current.text) + event.key.toLowerCase()
      typed.current = { text, at: now }
      const start = text.length === 1 ? index + 1 : index
      for (let step = 0; step < entries.length; step += 1) {
        const entry = entries[(start + step) % entries.length]
        if (entry?.textContent?.trim().toLowerCase().startsWith(text)) {
          event.preventDefault()
          entry.focus()
          return
        }
      }
    }
  }

  return createPortal(
    <div
      ref={list}
      role="menu"
      aria-label={label}
      tabIndex={-1}
      onKeyDown={moveFocus}
      style={{
        top: position?.top ?? 0,
        left: position?.left ?? 0,
        visibility: position === null ? 'hidden' : 'visible',
        ['--pop-from' as string]: position?.flipped ? '4px' : '-4px',
      }}
      className={`fixed z-(--z-popover) flex min-w-50 max-w-80 flex-col rounded-lg border border-line-strong bg-popover p-1 shadow-pop ring-1 ring-contrast ${position === null ? '' : 'hm-pop'}`}
    >
      {items.map((item, index) => {
        if (item.type === 'separator') {
          return (
            // biome-ignore lint/suspicious/noArrayIndexKey: separators have nothing else to tell them apart.
            <hr key={`separator-${index}`} className="-mx-1 my-1 h-px border-0 bg-line" />
          )
        }
        if (item.type === 'label') {
          return (
            <div
              key={`label-${item.label}`}
              role="presentation"
              className="flex h-6 items-center px-2 text-2xs font-semibold tracking-[0.06em] text-muted uppercase"
            >
              {item.label}
            </div>
          )
        }
        const Icon = item.icon
        return (
          <button
            key={item.label}
            type="button"
            {...(item.checked === undefined
              ? { role: 'menuitem' }
              : { role: 'menuitemcheckbox', 'aria-checked': item.checked })}
            disabled={item.disabled}
            onClick={() => {
              onClose(true)
              item.onSelect()
            }}
            className={`flex h-7 items-center gap-2 rounded px-2 text-left hover:bg-hover focus:bg-hover focus:outline-none disabled:opacity-50 ${item.danger === true ? 'text-danger-text' : 'text-ink'}`}
          >
            {Icon !== undefined ? (
              <Icon
                size={ICON.sm}
                aria-hidden="true"
                className={`shrink-0 ${item.danger === true ? '' : 'text-ink-2'}`}
              />
            ) : item.checked !== undefined ? (
              <Check
                size={ICON.sm}
                aria-hidden="true"
                className={`shrink-0 ${item.checked ? '' : 'invisible'}`}
              />
            ) : null}
            <span className="min-w-0 flex-1 truncate">{item.label}</span>
            {item.shortcut !== undefined && (
              <span className="shrink-0 pl-4 font-mono text-xs text-muted">
                {formatShortcut(item.shortcut)}
              </span>
            )}
          </button>
        )
      })}
    </div>,
    document.body,
  )
}

type MenuProps = {
  // The trigger's name for screen readers, for example "Actions for T-0004".
  label: string
  items: readonly MenuItem[]
  // Shown only while the row around it is hovered or focused, or the menu is open.
  revealOnHover?: boolean
  // The button defaults to "..." at 24px.
  icon?: LucideIcon
  size?: IconButtonSize
  tipSide?: 'top' | 'bottom'
  className?: string
}

// A "…" button with a list of actions.
export function Menu({
  label,
  items,
  revealOnHover = false,
  icon = Ellipsis,
  size = 'xs',
  tipSide,
  className = '',
}: MenuProps) {
  const [open, setOpen] = useState<{ anchor: Anchor; focus: 'first' | 'last' } | null>(null)
  const trigger = useRef<HTMLButtonElement>(null)

  const show = (focus: 'first' | 'last') => {
    if (trigger.current) {
      setOpen({ anchor: { kind: 'rect', rect: trigger.current.getBoundingClientRect() }, focus })
    }
  }
  const close = (restoreFocus: boolean) => {
    setOpen(null)
    if (restoreFocus) {
      trigger.current?.focus()
    }
  }
  // A new function each render would restart the popup's listeners, so it is kept in a ref.
  const closeRef = useRef(close)
  closeRef.current = close
  const stableClose = useRef((restoreFocus: boolean) => closeRef.current(restoreFocus)).current

  return (
    <>
      <IconButton
        ref={trigger}
        label={label}
        icon={icon}
        size={size}
        tipSide={tipSide}
        aria-haspopup="menu"
        aria-expanded={open !== null}
        onClick={(event) => {
          event.stopPropagation()
          if (open === null) {
            show('first')
          } else {
            close(false)
          }
        }}
        onKeyDown={(event) => {
          if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
            event.preventDefault()
            show(event.key === 'ArrowUp' ? 'last' : 'first')
          }
        }}
        className={`${
          revealOnHover && open === null
            ? 'opacity-0 group-focus-within:opacity-100 group-hover:opacity-100 focus-visible:opacity-100 pointer-coarse:opacity-100'
            : ''
        } ${className}`}
      />
      {open !== null && (
        <MenuPopup
          label={label}
          items={items}
          anchor={open.anchor}
          focus={open.focus}
          onClose={stableClose}
          ignore={trigger}
        />
      )}
    </>
  )
}

type ContextMenuState = {
  open: {
    label: string
    items: readonly MenuItem[]
    anchor: Anchor
    opener: HTMLElement | null
  } | null
}

const useContextMenu = create<ContextMenuState>()(() => ({ open: null }))

type OpenerEvent = {
  preventDefault: () => void
  clientX?: number
  clientY?: number
  currentTarget?: EventTarget | null
}

// Opens a menu at the pointer for a right click, or under the focused row for Shift+F10 and the
// context menu key. Mount one ContextMenuHost for the page.
export function openContextMenu(event: OpenerEvent, items: readonly MenuItem[], label = 'Actions') {
  event.preventDefault()
  const element = event.currentTarget instanceof HTMLElement ? event.currentTarget : null
  const fromKeyboard =
    event.clientX === undefined || (event.clientX === 0 && event.clientY === 0 && element !== null)
  const anchor: Anchor =
    fromKeyboard && element !== null
      ? { kind: 'rect', rect: element.getBoundingClientRect() }
      : { kind: 'point', x: event.clientX ?? 0, y: event.clientY ?? 0 }
  const opener = document.activeElement instanceof HTMLElement ? document.activeElement : element
  useContextMenu.setState({ open: { label, items, anchor, opener } })
}

export function ContextMenuHost() {
  const open = useContextMenu((state) => state.open)
  const close = useRef((restoreFocus: boolean) => {
    const current = useContextMenu.getState().open
    useContextMenu.setState({ open: null })
    if (restoreFocus) {
      current?.opener?.focus()
    }
  }).current
  if (open === null) {
    return null
  }
  return (
    <MenuPopup
      label={open.label}
      items={open.items}
      anchor={open.anchor}
      focus="first"
      onClose={close}
    />
  )
}
