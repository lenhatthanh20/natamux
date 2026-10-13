// How long to wait where the browser has no requestIdleCallback (Safari).
export const PREFETCH_FALLBACK_MS = 2000

// The longest wait for an idle moment. A page that is never idle, such as one with many live
// tasks, still loads the chunk before the user is likely to need it.
export const PREFETCH_TIMEOUT_MS = 3000

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
    const handle = requestIdleCallback(run, { timeout: PREFETCH_TIMEOUT_MS })
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
