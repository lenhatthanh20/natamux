import { useCallback, useLayoutEffect, useState } from 'react'

// The width of an element, kept up to date. Null until it is measured, and always null where
// ResizeObserver is missing (tests), so callers show the wide layout then. The first measure is in
// a layout effect, so the first paint already has the width.
export function useElementWidth<T extends HTMLElement>() {
  const [element, setElement] = useState<T | null>(null)
  const [width, setWidth] = useState<number | null>(null)
  const ref = useCallback((node: T | null) => setElement(node), [])

  useLayoutEffect(() => {
    if (element === null || typeof ResizeObserver === 'undefined') {
      return
    }
    const observer = new ResizeObserver(() => setWidth(element.clientWidth))
    observer.observe(element)
    setWidth(element.clientWidth)
    return () => observer.disconnect()
  }, [element])

  return [ref, width] as const
}
