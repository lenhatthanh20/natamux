import { act, cleanup, render, screen } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { NOW_INTERVAL_MS, useNow, useNowValue } from './useNow'

const renders = { page: 0, time: 0, late: 0 }

function Time() {
  renders.time += 1
  return <p>{useNow().toISOString()}</p>
}

// True from two minutes past nine.
function Late() {
  renders.late += 1
  const late = useNowValue((now) => now.getUTCMinutes() >= 2)
  return <p>{late ? 'Late' : 'Early'}</p>
}

function Page() {
  renders.page += 1
  return (
    <>
      <Time />
      <Time />
      <Late />
    </>
  )
}

const tick = (times = 1) => act(() => vi.advanceTimersByTime(NOW_INTERVAL_MS * times))

describe('useNow', () => {
  beforeEach(() => {
    vi.useFakeTimers({ now: new Date('2026-10-10T09:00:00.000Z') })
    renders.page = 0
    renders.time = 0
    renders.late = 0
  })

  afterEach(() => {
    cleanup()
    vi.useRealTimers()
    Reflect.deleteProperty(document, 'visibilityState')
  })

  it('re-renders only the parts that read the clock', () => {
    render(<Page />)
    expect(screen.getAllByText('2026-10-10T09:00:00.000Z')).toHaveLength(2)
    const time = renders.time

    tick()
    expect(screen.getAllByText('2026-10-10T09:00:30.000Z')).toHaveLength(2)
    expect(renders.page).toBe(1)
    expect(renders.time).toBe(time + 2)
  })

  it('re-renders a value read from the clock only when it changes', () => {
    render(<Page />)
    const late = renders.late

    tick(3)
    expect(screen.getByText('Early')).toBeTruthy()
    expect(renders.late).toBe(late)

    tick()
    expect(screen.getByText('Late')).toBeTruthy()
    expect(renders.late).toBe(late + 1)
  })

  it('keeps ticking while the page is hidden', () => {
    Object.defineProperty(document, 'visibilityState', { configurable: true, get: () => 'hidden' })
    render(<Page />)

    tick(3)
    expect(screen.getAllByText('2026-10-10T09:01:30.000Z')).toHaveLength(2)
    expect(screen.getByText('Early')).toBeTruthy()
    tick()
    expect(screen.getByText('Late')).toBeTruthy()
  })

  it('gives the first reader after a pause the time again', () => {
    const first = render(<Page />)
    first.unmount()
    act(() => vi.advanceTimersByTime(5 * 60_000))

    render(<Page />)
    expect(screen.getAllByText('2026-10-10T09:05:00.000Z')).toHaveLength(2)
    expect(screen.getByText('Late')).toBeTruthy()
  })
})
