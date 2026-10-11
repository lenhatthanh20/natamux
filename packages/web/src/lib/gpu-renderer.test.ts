import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { gpuRenderer, RELEASE_DELAY_MS, watchVisible } from './gpu-renderer'

function fakeLoad() {
  const addons: { disposed: boolean; lose: () => void }[] = []
  const load = vi.fn((onLost: () => void) => {
    const addon = {
      disposed: false,
      lose: onLost,
      dispose() {
        addon.disposed = true
      },
    }
    addons.push(addon)
    return addon
  })
  return { load, addons }
}

describe('gpuRenderer', () => {
  beforeEach(() => vi.useFakeTimers())
  afterEach(() => vi.useRealTimers())

  it('draws with the GPU from the start', () => {
    const { load, addons } = fakeLoad()
    gpuRenderer(load)
    expect(addons).toHaveLength(1)
    expect(addons[0]?.disposed).toBe(false)
  })

  it('gives the GPU back once the terminal has been hidden for a while', () => {
    const { load, addons } = fakeLoad()
    const renderer = gpuRenderer(load)
    renderer.setVisible(false)
    vi.advanceTimersByTime(RELEASE_DELAY_MS - 1)
    expect(addons[0]?.disposed).toBe(false)
    vi.advanceTimersByTime(1)
    expect(addons[0]?.disposed).toBe(true)
  })

  it('keeps the GPU through a quick switch away and back', () => {
    const { load, addons } = fakeLoad()
    const renderer = gpuRenderer(load)
    renderer.setVisible(false)
    vi.advanceTimersByTime(RELEASE_DELAY_MS / 2)
    renderer.setVisible(true)
    vi.advanceTimersByTime(RELEASE_DELAY_MS * 2)
    expect(addons).toHaveLength(1)
    expect(addons[0]?.disposed).toBe(false)
  })

  it('takes the GPU again when the terminal shows', () => {
    const { load, addons } = fakeLoad()
    const renderer = gpuRenderer(load)
    renderer.setVisible(false)
    vi.advanceTimersByTime(RELEASE_DELAY_MS)
    renderer.setVisible(true)
    expect(addons).toHaveLength(2)
    expect(addons[1]?.disposed).toBe(false)
  })

  it('does not load twice while it is shown', () => {
    const { load } = fakeLoad()
    const renderer = gpuRenderer(load)
    renderer.setVisible(true)
    renderer.setVisible(true)
    expect(load).toHaveBeenCalledTimes(1)
  })

  it('stays on the DOM when WebGL cannot start, and tries again at the next show', () => {
    const load = vi.fn(() => null)
    const renderer = gpuRenderer(load)
    renderer.setVisible(false)
    vi.advanceTimersByTime(RELEASE_DELAY_MS)
    renderer.setVisible(true)
    expect(load).toHaveBeenCalledTimes(2)
  })

  it('drops a renderer whose context was lost, and takes a new one at the next show', () => {
    const { load, addons } = fakeLoad()
    const renderer = gpuRenderer(load)
    addons[0]?.lose()
    expect(addons[0]?.disposed).toBe(true)
    renderer.setVisible(false)
    renderer.setVisible(true)
    expect(addons).toHaveLength(2)
  })

  it('stops its timer when disposed', () => {
    const { load, addons } = fakeLoad()
    const renderer = gpuRenderer(load)
    renderer.setVisible(false)
    renderer.dispose()
    expect(addons[0]?.disposed).toBe(true)
    expect(vi.getTimerCount()).toBe(0)
  })
})

describe('watchVisible', () => {
  afterEach(() => vi.unstubAllGlobals())

  it('reports when the element shows and hides', () => {
    let report: (entries: { isIntersecting: boolean }[]) => void = () => undefined
    const disconnect = vi.fn()
    const observe = vi.fn()
    vi.stubGlobal(
      'IntersectionObserver',
      class {
        constructor(callback: typeof report) {
          report = callback
        }
        observe = observe
        disconnect = disconnect
      },
    )
    const element = document.createElement('div')
    const seen: boolean[] = []

    const stop = watchVisible(element, (visible) => seen.push(visible))
    report([{ isIntersecting: false }])
    report([{ isIntersecting: false }, { isIntersecting: true }])
    stop()

    expect(observe).toHaveBeenCalledWith(element)
    expect(seen).toEqual([false, true])
    expect(disconnect).toHaveBeenCalled()
  })

  it('does nothing where the browser has no IntersectionObserver', () => {
    vi.stubGlobal('IntersectionObserver', undefined)
    const seen: boolean[] = []
    const stop = watchVisible(document.createElement('div'), (visible) => seen.push(visible))
    stop()
    expect(seen).toEqual([])
  })
})
