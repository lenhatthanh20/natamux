import { useSyncExternalStore } from 'react'

// Relative times such as "5m" move on this often.
export const NOW_INTERVAL_MS = 30_000

// One clock for the page. Each part that shows a relative time reads it itself, so a tick
// re-renders only those parts, not the whole dashboard. It stands still while the page is hidden
// and catches up when the page shows again.
let now = new Date()
let timer: ReturnType<typeof setInterval> | undefined
const listeners = new Set<() => void>()

function tick() {
  now = new Date()
  for (const listener of listeners) {
    listener()
  }
}

function runWhileShown() {
  clearInterval(timer)
  timer = undefined
  if (listeners.size > 0 && document.visibilityState !== 'hidden') {
    timer = setInterval(tick, NOW_INTERVAL_MS)
  }
}

function onVisibilityChange() {
  if (document.visibilityState !== 'hidden') {
    tick()
  }
  runWhileShown()
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener)
  if (listeners.size === 1) {
    // The clock stood still while nothing read it.
    now = new Date()
    document.addEventListener('visibilitychange', onVisibilityChange)
    runWhileShown()
  }
  return () => {
    listeners.delete(listener)
    if (listeners.size === 0) {
      document.removeEventListener('visibilitychange', onVisibilityChange)
      runWhileShown()
    }
  }
}

const readNow = () => now

export function useNow(): Date {
  return useSyncExternalStore(subscribe, readNow)
}

// What `read` makes of the clock. The caller re-renders only when that changes, so `read` returns
// a string, a number, a boolean or null, never a new object.
export function useNowValue<T>(read: (now: Date) => T): T {
  return useSyncExternalStore(subscribe, () => read(now))
}
