import { useSyncExternalStore } from 'react'

// Relative times such as "5m" move on this often.
export const NOW_INTERVAL_MS = 30_000

// One clock for the page. Each part that shows a relative time reads it itself, so a tick
// re-renders only those parts, not the whole dashboard. It ticks while the page is hidden too
// (the browser slows the timer down), so the needs-you count in the tab title stays right.
let now = new Date()
let timer: ReturnType<typeof setInterval> | undefined
const listeners = new Set<() => void>()

function tick() {
  now = new Date()
  for (const listener of listeners) {
    listener()
  }
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener)
  if (timer === undefined) {
    timer = setInterval(tick, NOW_INTERVAL_MS)
  }
  return () => {
    listeners.delete(listener)
    if (listeners.size === 0) {
      clearInterval(timer)
      timer = undefined
    }
  }
}

// The time of the clock. Without readers the clock stands still, so the first reader after a
// pause gets the time again. A second's step keeps two reads in one render the same.
export function clockNow(): Date {
  if (listeners.size === 0 && Math.abs(Date.now() - now.getTime()) >= 1000) {
    now = new Date()
  }
  return now
}

export function useNow(): Date {
  return useSyncExternalStore(subscribe, clockNow)
}

// What `read` makes of the clock. The caller re-renders only when that changes, so `read` returns
// a string, a number, a boolean or null, never a new object.
export function useNowValue<T>(read: (now: Date) => T): T {
  return useSyncExternalStore(subscribe, () => read(clockNow()))
}
