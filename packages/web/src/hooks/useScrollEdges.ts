import { useCallback, useEffect, useState } from 'react'

// Whether there is more to scroll to on the left and on the right, for the edge fades.
export function useScrollEdges<T extends HTMLElement>() {
  const [element, setElement] = useState<T | null>(null)
  const [edges, setEdges] = useState({ left: false, right: false })
  const ref = useCallback((node: T | null) => setElement(node), [])

  useEffect(() => {
    if (element === null) {
      return
    }
    const update = () => {
      const left = element.scrollLeft > 1
      const right = element.scrollLeft + element.clientWidth < element.scrollWidth - 1
      setEdges((current) => (current.left === left && current.right === right ? current : { left, right }))
    }
    update()
    element.addEventListener('scroll', update, { passive: true })
    const observer = typeof ResizeObserver === 'undefined' ? null : new ResizeObserver(update)
    observer?.observe(element)
    for (const child of element.children) {
      observer?.observe(child)
    }
    return () => {
      element.removeEventListener('scroll', update)
      observer?.disconnect()
    }
  }, [element])

  return [ref, edges] as const
}
