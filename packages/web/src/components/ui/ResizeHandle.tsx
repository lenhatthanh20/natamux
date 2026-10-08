import { type KeyboardEvent, type PointerEvent, useState } from 'react'

const KEY_STEP = 16

function savedWidth(key: string, initial: number, min: number, max: number): number {
  try {
    const value = Number(globalThis.localStorage?.getItem(key))
    return Number.isFinite(value) && value >= min && value <= max ? value : initial
  } catch {
    return initial
  }
}

// A panel width the user can drag, kept per browser.
export function useStoredWidth(key: string, initial: number, min: number, max: number) {
  const [width, setWidth] = useState(() => savedWidth(key, initial, min, max))
  const update = (next: number) => {
    const clamped = Math.round(Math.min(max, Math.max(min, next)))
    setWidth(clamped)
    try {
      globalThis.localStorage?.setItem(key, String(clamped))
    } catch {
      // Not kept; the next visit starts at the default width.
    }
  }
  return { width, min, max, setWidth: update }
}

type ResizeHandleProps = {
  label: string
  width: number
  min: number
  max: number
  // The panel grows when the pointer moves right (a left panel) or left (a right panel).
  edge: 'right' | 'left'
  onResize: (width: number) => void
}

// The thin line between two panels. Drag it, or focus it and use the arrow keys.
export function ResizeHandle({ label, width, min, max, edge, onResize }: ResizeHandleProps) {
  const sign = edge === 'right' ? 1 : -1

  const startDrag = (event: PointerEvent<HTMLDivElement>) => {
    event.preventDefault()
    const startX = event.clientX
    const startWidth = width
    const handle = event.currentTarget
    handle.setPointerCapture(event.pointerId)
    const move = (moved: globalThis.PointerEvent) => {
      onResize(startWidth + sign * (moved.clientX - startX))
    }
    const stop = () => {
      handle.removeEventListener('pointermove', move)
      handle.removeEventListener('pointerup', stop)
      document.body.style.cursor = ''
    }
    document.body.style.cursor = 'col-resize'
    handle.addEventListener('pointermove', move)
    handle.addEventListener('pointerup', stop)
  }

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key === 'ArrowRight' || event.key === 'ArrowLeft') {
      event.preventDefault()
      onResize(width + (event.key === 'ArrowRight' ? sign : -sign) * KEY_STEP)
    }
  }

  return (
    // biome-ignore lint/a11y/useSemanticElements: a focusable separator that resizes, which <hr> cannot be.
    <div
      role="separator"
      aria-label={label}
      aria-orientation="vertical"
      aria-valuenow={width}
      aria-valuemin={min}
      aria-valuemax={max}
      tabIndex={0}
      onPointerDown={startDrag}
      onKeyDown={onKeyDown}
      className="group relative z-10 -mx-[3px] w-[7px] shrink-0 cursor-col-resize touch-none focus-visible:outline-none"
    >
      <span className="absolute inset-y-0 left-[3px] w-px bg-line transition-colors group-hover:bg-accent/60 group-focus-visible:bg-accent group-active:bg-accent" />
    </div>
  )
}
