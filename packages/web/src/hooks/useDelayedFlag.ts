import { useEffect, useRef, useState } from 'react'

// Turns a loading flag into one for a skeleton: it shows only after `delay` ms of loading (no
// flash for fast loads) and then stays at least `minimum` ms (no blink when data arrives).
export function useDelayedFlag(active: boolean, delay = 200, minimum = 300): boolean {
  const [shown, setShown] = useState(false)
  const shownAt = useRef(0)

  useEffect(() => {
    if (active && !shown) {
      const timer = setTimeout(() => {
        shownAt.current = Date.now()
        setShown(true)
      }, delay)
      return () => clearTimeout(timer)
    }
    if (!active && shown) {
      const timer = setTimeout(
        () => setShown(false),
        Math.max(0, minimum - (Date.now() - shownAt.current)),
      )
      return () => clearTimeout(timer)
    }
  }, [active, shown, delay, minimum])

  return shown
}
