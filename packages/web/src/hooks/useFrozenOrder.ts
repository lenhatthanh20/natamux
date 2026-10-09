import { type PointerEvent, useEffect, useRef, useState } from 'react'
import { freezeOrder } from '../lib/frozen-order'

const HOLD_MS = 2000

// Keeps rows where they are while the user is working on them: with the mouse over the list, and
// for two seconds after a key moved the selection. Live updates still show, but a row never jumps
// away from the pointer. `resetKey` changes when the user asks for another order (a sort click, a
// filter), and then the new order applies at once.
export function useFrozenOrder<T extends { id: string }>(items: readonly T[], resetKey: string) {
  const [pointerInside, setPointerInside] = useState(false)
  const [held, setHeld] = useState(false)
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)
  const shown = useRef<{ ids: string[]; resetKey: string }>({ ids: [], resetKey })

  const frozen = (pointerInside || held) && shown.current.resetKey === resetKey
  const ordered = frozen ? freezeOrder(shown.current.ids, items) : [...items]

  useEffect(() => {
    shown.current = { ids: ordered.map((item) => item.id), resetKey }
  })
  useEffect(() => () => clearTimeout(timer.current), [])

  return {
    items: ordered,
    // Put these on the element around the list.
    handlers: {
      // Touch has no hover, and no leave event to end it.
      onPointerEnter: (event: PointerEvent) => setPointerInside(event.pointerType !== 'touch'),
      onPointerLeave: () => setPointerInside(false),
    },
    // Call it when a key moved the selection.
    hold: () => {
      setHeld(true)
      clearTimeout(timer.current)
      timer.current = setTimeout(() => setHeld(false), HOLD_MS)
    },
  }
}
