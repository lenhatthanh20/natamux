// How long to wait where the browser has no requestIdleCallback (Safari).
export const PREFETCH_FALLBACK_MS = 2000

// Loads a lazy chunk once the page is idle, so the first view that needs it does not wait for
// the download. A failed load is ignored here: the lazy view shows its own error later. Returns a
// function that cancels the load if it has not started.
export function prefetchWhenIdle(load: () => Promise<unknown>): () => void {
  let done = false
  const run = () => {
    if (done) {
      return
    }
    done = true
    load().catch(() => undefined)
  }
  if (typeof requestIdleCallback === 'function') {
    const handle = requestIdleCallback(run)
    return () => {
      done = true
      cancelIdleCallback(handle)
    }
  }
  const timer = setTimeout(run, PREFETCH_FALLBACK_MS)
  return () => {
    done = true
    clearTimeout(timer)
  }
}
