import { useEffect, useId, useLayoutEffect, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import { formatShortcut } from '../../lib/keys'

type Side = 'top' | 'bottom'

type Shown = { el: HTMLElement; text: string; kbd: string | null; side: Side }

const SHOW_DELAY_MS = 400
// Moving to the next trigger this soon after one closed shows the next at once.
const WARM_MS = 600
const GAP = 6
const EDGE = 8
// Keys that move the focus. A focus within this time of one of them came from the user.
const MOVE_KEYS = /^(Tab|Arrow(Up|Down|Left|Right)|Home|End)$/
const MOVE_KEY_MS = 150
const SELECTOR = '[data-tip], [data-tip-overflow]'

function isFocusVisible(el: HTMLElement): boolean {
  try {
    return el.matches(':focus-visible')
  } catch {
    return false
  }
}

// Reads what a trigger wants to say. Null when it has nothing to show right now.
function describe(el: HTMLElement): Shown | null {
  const text = el.dataset.tip ?? el.textContent?.trim() ?? ''
  if (text === '') {
    return null
  }
  // Truncated text gets a tooltip only while it really is cut: to one line (wider than its box) or
  // to a few lines by a line clamp (taller than its box).
  const cut = el.scrollWidth > el.clientWidth || el.scrollHeight > el.clientHeight + 1
  if (el.hasAttribute('data-tip-overflow') && !cut) {
    return null
  }
  return {
    el,
    text,
    kbd: el.dataset.tipKbd ?? null,
    side: el.dataset.tipSide === 'bottom' ? 'bottom' : 'top',
  }
}

// One tooltip for the whole page. Elements opt in with data-tip, data-tip-kbd, data-tip-side and
// data-tip-overflow, so no component has to hold tooltip state.
export function Tooltip() {
  const id = useId()
  const [shown, setShown] = useState<Shown | null>(null)
  const [position, setPosition] = useState<{ top: number; left: number; side: Side } | null>(null)
  const box = useRef<HTMLDivElement>(null)
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)
  const pending = useRef<HTMLElement | null>(null)
  const lastClosed = useRef(0)
  const moveKeyAt = useRef(0)
  const visible = useRef<Shown | null>(null)
  visible.current = shown

  useEffect(() => {
    const cancel = () => {
      clearTimeout(timer.current)
      pending.current = null
    }
    const hide = () => {
      cancel()
      if (visible.current !== null) {
        lastClosed.current = Date.now()
        setPosition(null)
        setShown(null)
      }
    }
    const show = (el: HTMLElement) => {
      const next = describe(el)
      if (next !== null) {
        setPosition(null)
        setShown(next)
      }
    }
    const trigger = (target: EventTarget | null): HTMLElement | null =>
      target instanceof Element ? target.closest<HTMLElement>(SELECTOR) : null

    const onOver = (event: PointerEvent) => {
      if (event.pointerType === 'touch') {
        return
      }
      const el = trigger(event.target)
      if (el === (visible.current?.el ?? pending.current)) {
        return
      }
      hide()
      if (el === null) {
        return
      }
      if (Date.now() - lastClosed.current < WARM_MS) {
        show(el)
        return
      }
      pending.current = el
      timer.current = setTimeout(() => {
        pending.current = null
        show(el)
      }, SHOW_DELAY_MS)
    }
    const onFocus = (event: FocusEvent) => {
      const el = trigger(event.target)
      // Focus that the page restores (after Escape, a closed dialog) is not the user moving.
      const moved = Date.now() - moveKeyAt.current < MOVE_KEY_MS
      if (el !== null && moved && isFocusVisible(el)) {
        hide()
        show(el)
      }
    }
    // Escape closes the tooltip before anything else.
    const onKey = (event: KeyboardEvent) => {
      if (MOVE_KEYS.test(event.key)) {
        moveKeyAt.current = Date.now()
      }
      if (event.key === 'Escape' && visible.current !== null) {
        event.stopPropagation()
      }
      hide()
    }

    document.addEventListener('pointerover', onOver)
    document.addEventListener('pointerdown', hide, true)
    document.addEventListener('focusin', onFocus)
    document.addEventListener('focusout', hide)
    document.addEventListener('keydown', onKey, true)
    document.addEventListener('scroll', hide, true)
    document.documentElement.addEventListener('pointerleave', hide)
    return () => {
      cancel()
      document.removeEventListener('pointerover', onOver)
      document.removeEventListener('pointerdown', hide, true)
      document.removeEventListener('focusin', onFocus)
      document.removeEventListener('focusout', hide)
      document.removeEventListener('keydown', onKey, true)
      document.removeEventListener('scroll', hide, true)
      document.documentElement.removeEventListener('pointerleave', hide)
    }
  }, [])

  // Measure first, then place: the tooltip flips to the other side when its side has no room.
  useLayoutEffect(() => {
    if (shown === null || box.current === null) {
      return
    }
    const anchor = shown.el.getBoundingClientRect()
    const size = box.current.getBoundingClientRect()
    const fitsTop = anchor.top - GAP - size.height >= EDGE
    const fitsBottom = anchor.bottom + GAP + size.height <= window.innerHeight - EDGE
    const side: Side =
      shown.side === 'top'
        ? fitsTop || !fitsBottom
          ? 'top'
          : 'bottom'
        : fitsBottom || !fitsTop
          ? 'bottom'
          : 'top'
    const left = Math.max(
      EDGE,
      Math.min(
        anchor.left + anchor.width / 2 - size.width / 2,
        window.innerWidth - size.width - EDGE,
      ),
    )
    setPosition({
      top: side === 'top' ? anchor.top - GAP - size.height : anchor.bottom + GAP,
      left,
      side,
    })
  }, [shown])

  // The trigger is described by the tooltip only when it says more than the trigger's own name.
  useEffect(() => {
    if (shown === null) {
      return
    }
    const adds = shown.kbd !== null || shown.text !== shown.el.getAttribute('aria-label')
    if (adds) {
      shown.el.setAttribute('aria-describedby', id)
    }
    return () => shown.el.removeAttribute('aria-describedby')
  }, [shown, id])

  if (shown === null) {
    return null
  }
  return createPortal(
    <div
      ref={box}
      id={id}
      role="tooltip"
      style={{
        top: position?.top ?? 0,
        left: position?.left ?? 0,
        visibility: position === null ? 'hidden' : 'visible',
        ['--pop-from' as string]: position?.side === 'bottom' ? '-4px' : '4px',
      }}
      className={`pointer-events-none fixed z-(--z-tooltip) flex max-w-70 items-center gap-2 rounded border border-transparent bg-tooltip-bg px-2 py-1 text-xs text-tooltip-fg shadow-pop dark:border-line-strong ${position === null ? '' : 'hm-pop'}`}
    >
      <span className="min-w-0 break-words">{shown.text}</span>
      {shown.kbd !== null && (
        <span className="shrink-0 font-mono text-2xs opacity-70">{formatShortcut(shown.kbd)}</span>
      )}
    </div>,
    document.body,
  )
}
