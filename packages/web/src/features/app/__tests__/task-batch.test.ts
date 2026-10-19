import type { Task } from '@natamux/core'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { BATCH_FALLBACK_MS, createTaskBatch } from '../task-batch'

const task = (id: string) => ({ id }) as Task

// A requestAnimationFrame that a test runs by hand. A hidden tab never runs it.
function frames() {
  const callbacks = new Map<number, FrameRequestCallback>()
  let last = 0
  vi.stubGlobal(
    'requestAnimationFrame',
    vi.fn((callback: FrameRequestCallback) => {
      last += 1
      callbacks.set(last, callback)
      return last
    }),
  )
  vi.stubGlobal(
    'cancelAnimationFrame',
    vi.fn((handle: number) => callbacks.delete(handle)),
  )
  return () => {
    const due = [...callbacks.values()]
    callbacks.clear()
    for (const callback of due) {
      callback(0)
    }
  }
}

beforeEach(() => {
  vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] })
})

afterEach(() => {
  vi.useRealTimers()
  vi.unstubAllGlobals()
})

describe('createTaskBatch', () => {
  it('writes a burst of changes once, on the next frame, in the order they came', () => {
    const runFrame = frames()
    const apply = vi.fn()
    const batch = createTaskBatch(apply)

    batch.push(task('T-0001'))
    batch.push({ deleted: 'T-0002' })
    batch.push(task('T-0003'))
    expect(apply).not.toHaveBeenCalled()

    runFrame()
    expect(apply).toHaveBeenCalledTimes(1)
    expect(apply).toHaveBeenCalledWith([task('T-0001'), { deleted: 'T-0002' }, task('T-0003')])

    // The timer of that batch does not write again.
    vi.advanceTimersByTime(BATCH_FALLBACK_MS)
    expect(apply).toHaveBeenCalledTimes(1)
  })

  it('still writes in a hidden tab, where no frame runs', () => {
    frames()
    const apply = vi.fn()
    const batch = createTaskBatch(apply)

    batch.push(task('T-0001'))
    batch.push(task('T-0002'))
    vi.advanceTimersByTime(BATCH_FALLBACK_MS)

    expect(apply).toHaveBeenCalledTimes(1)
    expect(apply).toHaveBeenCalledWith([task('T-0001'), task('T-0002')])
  })

  it('writes at once on flush, and starts a new batch after it', () => {
    const runFrame = frames()
    const apply = vi.fn()
    const batch = createTaskBatch(apply)

    batch.push(task('T-0001'))
    batch.push({ deleted: 'T-0001' })
    batch.flush()
    expect(apply).toHaveBeenCalledWith([task('T-0001'), { deleted: 'T-0001' }])

    runFrame()
    batch.flush()
    expect(apply).toHaveBeenCalledTimes(1)

    batch.push(task('T-0002'))
    runFrame()
    expect(apply).toHaveBeenCalledTimes(2)
    expect(apply).toHaveBeenLastCalledWith([task('T-0002')])
  })

  it('drops the queued changes when cancelled', () => {
    const runFrame = frames()
    const apply = vi.fn()
    const batch = createTaskBatch(apply)

    batch.push(task('T-0001'))
    batch.cancel()
    runFrame()
    vi.advanceTimersByTime(BATCH_FALLBACK_MS)

    expect(apply).not.toHaveBeenCalled()
  })
})
