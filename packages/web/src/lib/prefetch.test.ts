import { afterEach, describe, expect, it, vi } from 'vitest'
import { PREFETCH_FALLBACK_MS, prefetchWhenIdle } from './prefetch'

// A requestIdleCallback that a test runs by hand.
function idleBrowser() {
  const callbacks: (() => void)[] = []
  const cancel = vi.fn()
  vi.stubGlobal(
    'requestIdleCallback',
    vi.fn((callback: () => void) => {
      callbacks.push(callback)
      return callbacks.length
    }),
  )
  vi.stubGlobal('cancelIdleCallback', cancel)
  const runIdle = () => {
    for (const callback of callbacks.splice(0)) {
      callback()
    }
  }
  return { runIdle, cancel }
}

afterEach(() => {
  vi.unstubAllGlobals()
  vi.useRealTimers()
})

describe('prefetchWhenIdle', () => {
  it('loads when the browser is idle', () => {
    const { runIdle } = idleBrowser()
    const load = vi.fn(async () => undefined)
    prefetchWhenIdle(load)
    expect(load).not.toHaveBeenCalled()

    runIdle()
    expect(load).toHaveBeenCalledTimes(1)
  })

  it('waits a fixed time where the browser has no idle callback', () => {
    vi.useFakeTimers()
    vi.stubGlobal('requestIdleCallback', undefined)
    const load = vi.fn(async () => undefined)
    prefetchWhenIdle(load)

    vi.advanceTimersByTime(PREFETCH_FALLBACK_MS - 1)
    expect(load).not.toHaveBeenCalled()
    vi.advanceTimersByTime(1)
    expect(load).toHaveBeenCalledTimes(1)
    vi.advanceTimersByTime(PREFETCH_FALLBACK_MS * 5)
    expect(load).toHaveBeenCalledTimes(1)
  })

  it('loads once', () => {
    const { runIdle } = idleBrowser()
    const load = vi.fn(async () => undefined)
    const cancel = prefetchWhenIdle(load)
    runIdle()
    cancel()
    runIdle()
    expect(load).toHaveBeenCalledTimes(1)
  })

  it('stops when cancelled before the browser is idle', () => {
    const { runIdle, cancel: cancelIdle } = idleBrowser()
    const load = vi.fn(async () => undefined)
    const cancel = prefetchWhenIdle(load)
    cancel()
    expect(cancelIdle).toHaveBeenCalledWith(1)
    // A browser that runs the callback anyway still loads nothing.
    runIdle()
    expect(load).not.toHaveBeenCalled()
  })

  it('stops the timer when cancelled', () => {
    vi.useFakeTimers()
    vi.stubGlobal('requestIdleCallback', undefined)
    const load = vi.fn(async () => undefined)
    prefetchWhenIdle(load)()
    vi.advanceTimersByTime(PREFETCH_FALLBACK_MS * 2)
    expect(load).not.toHaveBeenCalled()
  })

  it('keeps a failed load to itself', async () => {
    const { runIdle } = idleBrowser()
    const load = vi.fn(() => Promise.reject(new Error('Offline.')))
    prefetchWhenIdle(load)
    expect(() => runIdle()).not.toThrow()
    await Promise.resolve()
    expect(load).toHaveBeenCalledTimes(1)
  })
})
