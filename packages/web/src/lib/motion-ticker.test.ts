import { afterEach, describe, expect, it, vi } from 'vitest'
import { MOTION_TICK_MS, startMotionTicker } from './motion-ticker'

function fakeAnimation(iterations: number, css = true) {
  return {
    ...(css ? { animationName: 'hm-spin' } : {}),
    playState: 'running' as AnimationPlayState,
    currentTime: 0 as CSSNumberish | null,
    effect: { getTiming: () => ({ iterations }) },
    pause() {
      this.playState = 'paused'
    },
  }
}

describe('motion ticker', () => {
  afterEach(() => {
    vi.useRealTimers()
    vi.restoreAllMocks()
  })

  it('moves endless CSS animations to the same tick and leaves the others alone', () => {
    vi.useFakeTimers()
    let clock = 1010
    vi.spyOn(performance, 'now').mockImplementation(() => clock)
    const spin = fakeAnimation(Number.POSITIVE_INFINITY)
    const stripes = fakeAnimation(Number.POSITIVE_INFINITY)
    const pop = fakeAnimation(1)
    const transition = fakeAnimation(Number.POSITIVE_INFINITY, false)
    const target = { getAnimations: () => [spin, stripes, pop, transition] } as unknown as Document

    const stop = startMotionTicker(target)
    expect(spin.playState).toBe('paused')
    expect(spin.currentTime).toBe(1000)
    expect(stripes.currentTime).toBe(1000)
    expect(pop.playState).toBe('running')
    expect(transition.playState).toBe('running')

    clock = 1140
    vi.advanceTimersByTime(MOTION_TICK_MS)
    expect(spin.currentTime).toBe(1125)
    expect(stripes.currentTime).toBe(1125)
    expect(pop.currentTime).toBe(0)
    expect(transition.currentTime).toBe(0)

    stop()
    clock = 2000
    vi.advanceTimersByTime(MOTION_TICK_MS * 3)
    expect(spin.currentTime).toBe(1125)
  })

  it('leaves the animations alone while the page is hidden', () => {
    vi.useFakeTimers()
    const spin = fakeAnimation(Number.POSITIVE_INFINITY)
    const target = {
      visibilityState: 'hidden',
      getAnimations: () => [spin],
    } as unknown as Document

    const stop = startMotionTicker(target)
    vi.advanceTimersByTime(MOTION_TICK_MS * 3)
    expect(spin.playState).toBe('running')
    expect(spin.currentTime).toBe(0)
    stop()
  })

  it('does nothing where the browser has no getAnimations', () => {
    const stop = startMotionTicker({} as Document)
    expect(() => stop()).not.toThrow()
  })
})
