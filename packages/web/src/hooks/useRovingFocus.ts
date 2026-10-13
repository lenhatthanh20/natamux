import { type FocusEvent, type KeyboardEvent, useRef, useState } from 'react'
import { moveTarget, typeAheadTarget } from '../lib/roving'

type Options = {
  // The keys of the items in the order they are shown. Hidden items are not in the list.
  keys: readonly string[]
  // The item that gets the tab stop when nothing has focus yet, for example the current one.
  preferred?: string | null
  // j and k move too.
  letters?: boolean
  // What each item says, for type-ahead. Leave out to turn type-ahead off.
  labels?: readonly string[]
  // Called for keys the list does not handle itself. Return true when the key was used.
  onKey?: (event: KeyboardEvent<HTMLElement>, key: string) => boolean
  // Called after a key moved focus, with the key it moved to.
  onMove?: (key: string) => void
}

const TYPE_AHEAD_MS = 600

// The item with this key inside a container.
export function findRoving(container: ParentNode | null, key: string): HTMLElement | null {
  return (
    [...(container?.querySelectorAll<HTMLElement>('[data-roving]') ?? [])].find(
      (element) => element.dataset.roving === key,
    ) ?? null
  )
}

// The props of one item. A memoized row builds them itself from the `stop` and `activate` that
// `useRovingFocus` returns, since `item(key)` is a new object on every render.
export function rovingItem(key: string, isStop: boolean, activate: (key: string) => void) {
  return {
    'data-roving': key,
    tabIndex: isStop ? 0 : -1,
    onFocus: (event: FocusEvent<HTMLElement>) => {
      if (event.target === event.currentTarget) {
        activate(key)
      }
    },
  }
}

// One tab stop for a whole list or tree. Arrow keys, Home and End move focus between the items;
// the item with focus is the one that has tabIndex 0. Give `container` to the element around the
// items, and `item(key)` to each item.
export function useRovingFocus({ keys, preferred, letters, labels, onKey, onMove }: Options) {
  const [active, setActive] = useState<string | null>(null)
  const container = useRef<HTMLElement | null>(null)
  const typed = useRef({ text: '', at: 0 })

  const stop =
    active !== null && keys.includes(active)
      ? active
      : preferred !== undefined && preferred !== null && keys.includes(preferred)
        ? preferred
        : (keys[0] ?? null)

  const focusKey = (key: string) => {
    setActive(key)
    findRoving(container.current, key)?.focus()
    onMove?.(key)
  }

  const onKeyDown = (event: KeyboardEvent<HTMLElement>) => {
    const element =
      event.target instanceof HTMLElement ? event.target.closest('[data-roving]') : null
    const key = element instanceof HTMLElement ? (element.dataset.roving ?? null) : null
    if (key === null || event.ctrlKey || event.metaKey || event.altKey) {
      return
    }
    if (onKey?.(event, key) === true) {
      event.preventDefault()
      event.stopPropagation()
      return
    }
    const index = keys.indexOf(key)
    const next = moveTarget(event.key, index, keys.length, letters)
    if (next !== null) {
      event.preventDefault()
      event.stopPropagation()
      const target = keys[next]
      if (target !== undefined) {
        focusKey(target)
      }
      return
    }
    if (labels !== undefined && event.key.length === 1 && event.key !== ' ' && !event.shiftKey) {
      const now = Date.now()
      const text = (now - typed.current.at > TYPE_AHEAD_MS ? '' : typed.current.text) + event.key
      typed.current = { text, at: now }
      const found = typeAheadTarget(labels, text, index)
      const target = found === -1 ? undefined : keys[found]
      if (target !== undefined) {
        event.preventDefault()
        focusKey(target)
      }
    }
  }

  return {
    container: (node: HTMLElement | null) => {
      container.current = node
    },
    focusKey,
    // Put these on the element around the items.
    containerProps: { onKeyDown },
    // Put these on each item.
    item: (key: string) => rovingItem(key, key === stop, setActive),
    // The key of the item with the tab stop.
    stop,
    activate: setActive,
  }
}
