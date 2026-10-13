import { afterEach, describe, expect, it, vi } from 'vitest'
import {
  createGpuBudget,
  GIVE_BACK_DELAY_MS,
  GPU_CONTEXTS,
  type GpuBudget,
  gpuRenderer,
  MAX_CONTEXT_LOSSES,
  type Renderer,
  watchVisible,
} from './gpu-renderer'
import { MAX_PANES } from './panes'

type Addon = { disposed: boolean; lose: () => void; dispose(): void }

function makeAddon(onLost: () => void): Addon {
  const addon: Addon = {
    disposed: false,
    lose: onLost,
    dispose() {
      addon.disposed = true
    },
  }
  return addon
}

function fakeLoad() {
  const addons: Addon[] = []
  const load = vi.fn((onLost: () => void) => {
    const addon = makeAddon(onLost)
    addons.push(addon)
    return addon
  })
  return { load, addons }
}

// A loader that answers later, like the real one when WebGL starts in a separate step.
function slowLoad() {
  const addons: Addon[] = []
  const pending: ((renderer: Renderer | null) => void)[] = []
  const onLostFor: (() => void)[] = []
  const load = vi.fn(
    (onLost: () => void) =>
      new Promise<Renderer | null>((resolve) => {
        pending.push((renderer) => {
          if (renderer !== null) {
            addons.push(renderer as Addon)
          }
          resolve(renderer)
        })
        onLostFor.push(onLost)
      }),
  )
  const finish = async (index = 0) => {
    const onLost = onLostFor[index]
    if (onLost === undefined) {
      throw new Error('No load to finish')
    }
    pending[index]?.(makeAddon(onLost))
    await flush()
  }
  return { load, addons, finish }
}

async function flush() {
  for (let i = 0; i < 5; i += 1) {
    await Promise.resolve()
  }
}

// One terminal with its own loader and switch counter, on the given budget.
function handle(budget: GpuBudget) {
  const loader = fakeLoad()
  const onSwitch = vi.fn()
  const renderer = gpuRenderer(loader.load, { onSwitch, budget })
  return { ...loader, onSwitch, renderer }
}

describe('gpuRenderer', () => {
  afterEach(() => vi.useRealTimers())

  it('draws with the GPU from the start', () => {
    const budget = createGpuBudget()
    const { addons, renderer } = handle(budget)
    expect(addons).toHaveLength(1)
    expect(addons[0]?.disposed).toBe(false)
    expect(renderer.hasRenderer()).toBe(true)
    expect(budget.inUse()).toBe(1)
  })

  it('does not load twice while it is shown', () => {
    const budget = createGpuBudget()
    const { load, renderer } = handle(budget)
    renderer.setVisible(true)
    renderer.setVisible(true)
    expect(load).toHaveBeenCalledTimes(1)
  })

  it('stays on the DOM when WebGL cannot start, and tries again at the next show', () => {
    const budget = createGpuBudget()
    const load = vi.fn(() => null)
    const renderer = gpuRenderer(load, { budget })
    expect(renderer.hasRenderer()).toBe(false)
    expect(budget.inUse()).toBe(0)
    renderer.setVisible(false)
    renderer.setVisible(true)
    expect(load).toHaveBeenCalledTimes(2)
  })

  it('treats a load that rejects like WebGL that cannot start', async () => {
    const budget = createGpuBudget()
    const load = vi.fn(() => Promise.reject(new Error('no webgl')))
    const renderer = gpuRenderer(load, { budget })
    await flush()
    expect(renderer.hasRenderer()).toBe(false)
    expect(budget.inUse()).toBe(0)
  })

  it('keeps the renderer when shown again before the give-back delay', () => {
    vi.useFakeTimers()
    const budget = createGpuBudget()
    const { addons, load, onSwitch, renderer } = handle(budget)
    renderer.setVisible(false)
    vi.advanceTimersByTime(GIVE_BACK_DELAY_MS - 1)
    renderer.setVisible(true)
    vi.advanceTimersByTime(60 * 60 * 1000)
    expect(addons[0]?.disposed).toBe(false)
    expect(renderer.hasRenderer()).toBe(true)
    expect(load).toHaveBeenCalledTimes(1)
    expect(onSwitch).toHaveBeenCalledTimes(1)
    expect(vi.getTimerCount()).toBe(0)
  })

  it('gives the renderer back after the delay, and takes one again at the next show', () => {
    vi.useFakeTimers()
    const budget = createGpuBudget()
    const { addons, onSwitch, renderer } = handle(budget)
    renderer.setVisible(false)
    vi.advanceTimersByTime(GIVE_BACK_DELAY_MS)
    expect(addons[0]?.disposed).toBe(true)
    expect(renderer.hasRenderer()).toBe(false)
    expect(budget.inUse()).toBe(0)
    expect(onSwitch).toHaveBeenCalledTimes(2)

    renderer.setVisible(true)
    expect(addons).toHaveLength(2)
    expect(addons[1]?.disposed).toBe(false)
    expect(renderer.hasRenderer()).toBe(true)
    expect(budget.inUse()).toBe(1)
  })

  it('stays on the DOM at the show after a give back when the budget is full of shown ones', () => {
    vi.useFakeTimers()
    const budget = createGpuBudget(1)
    const first = handle(budget)
    first.renderer.setVisible(false)
    vi.advanceTimersByTime(GIVE_BACK_DELAY_MS)
    const second = handle(budget)
    first.renderer.setVisible(true)
    expect(first.renderer.hasRenderer()).toBe(false)
    expect(second.renderer.hasRenderer()).toBe(true)
  })

  it('drops the give-back timer on dispose', () => {
    vi.useFakeTimers()
    const budget = createGpuBudget()
    const { onSwitch, renderer } = handle(budget)
    renderer.setVisible(false)
    renderer.dispose()
    expect(vi.getTimerCount()).toBe(0)
    expect(onSwitch).toHaveBeenCalledTimes(1)
  })

  it('loads once per terminal when 6 terminals are shown and hidden 10 times (AC 33)', () => {
    const budget = createGpuBudget()
    const handles = Array.from({ length: GPU_CONTEXTS }, () => handle(budget))
    for (let round = 0; round < 10; round += 1) {
      for (const one of handles) {
        one.renderer.setVisible(false)
        one.renderer.setVisible(true)
      }
    }
    const loads = handles.reduce((total, one) => total + one.load.mock.calls.length, 0)
    expect(loads).toBe(GPU_CONTEXTS)
    expect(budget.inUse()).toBe(GPU_CONTEXTS)
  })

  it('takes the renderer of the terminal hidden longest when the budget is full (AC 33)', () => {
    const budget = createGpuBudget()
    const handles = Array.from({ length: GPU_CONTEXTS }, () => handle(budget))
    // Hide in a mixed order: index 3 first, so it has been hidden longest.
    for (const index of [3, 0, 5, 1, 4, 2]) {
      handles[index]?.renderer.setVisible(false)
    }
    const seventh = handle(budget)

    expect(handles[3]?.addons[0]?.disposed).toBe(true)
    expect(handles[3]?.renderer.hasRenderer()).toBe(false)
    expect(handles[3]?.onSwitch).toHaveBeenCalledTimes(2)
    for (const index of [0, 1, 2, 4, 5]) {
      expect(handles[index]?.addons[0]?.disposed).toBe(false)
      expect(handles[index]?.renderer.hasRenderer()).toBe(true)
    }
    expect(seventh.addons).toHaveLength(1)
    expect(seventh.renderer.hasRenderer()).toBe(true)
    expect(budget.inUse()).toBe(GPU_CONTEXTS)
  })

  it('stays on the DOM when all 6 are visible and a 7th comes', () => {
    const budget = createGpuBudget()
    const handles = Array.from({ length: GPU_CONTEXTS }, () => handle(budget))
    const seventh = handle(budget)
    expect(seventh.load).not.toHaveBeenCalled()
    expect(seventh.renderer.hasRenderer()).toBe(false)
    for (const one of handles) {
      expect(one.addons[0]?.disposed).toBe(false)
    }
    // No retry until the next show.
    seventh.renderer.setVisible(true)
    expect(seventh.load).not.toHaveBeenCalled()
  })

  it('lets the evicted terminal take a renderer again when it shows and one is hidden', () => {
    const budget = createGpuBudget(2)
    const first = handle(budget)
    const second = handle(budget)
    first.renderer.setVisible(false)
    const third = handle(budget)
    expect(first.renderer.hasRenderer()).toBe(false)
    second.renderer.setVisible(false)
    first.renderer.setVisible(true)
    expect(first.renderer.hasRenderer()).toBe(true)
    expect(second.renderer.hasRenderer()).toBe(false)
    expect(third.renderer.hasRenderer()).toBe(true)
  })

  it('frees the slot of a disposed terminal, so a new one loads with no eviction', () => {
    const budget = createGpuBudget(1)
    const first = handle(budget)
    first.renderer.dispose()
    expect(first.addons[0]?.disposed).toBe(true)
    expect(budget.inUse()).toBe(0)
    const second = handle(budget)
    expect(second.addons).toHaveLength(1)
    expect(first.onSwitch).toHaveBeenCalledTimes(1)
  })

  it('does not call onSwitch or load again after dispose', () => {
    const budget = createGpuBudget()
    const one = handle(budget)
    one.renderer.dispose()
    one.renderer.dispose()
    one.renderer.setVisible(true)
    expect(one.load).toHaveBeenCalledTimes(1)
    expect(one.onSwitch).toHaveBeenCalledTimes(1)
  })

  it('shares the default budget when none is given', () => {
    const load = vi.fn(() => null)
    const renderer = gpuRenderer(load)
    expect(renderer.hasRenderer()).toBe(false)
    expect(load).toHaveBeenCalledTimes(1)
  })

  it('disposes an async renderer that arrives after dispose (AC 34)', async () => {
    const budget = createGpuBudget()
    const { load, addons, finish } = slowLoad()
    const renderer = gpuRenderer(load, { budget })
    expect(budget.inUse()).toBe(1)
    renderer.dispose()
    expect(budget.inUse()).toBe(0)
    await finish()
    expect(addons).toHaveLength(1)
    expect(addons[0]?.disposed).toBe(true)
    expect(budget.inUse()).toBe(0)
  })

  it('disposes an async renderer that arrives while hidden (AC 34)', async () => {
    const budget = createGpuBudget()
    const { load, addons, finish } = slowLoad()
    const onSwitch = vi.fn()
    const renderer = gpuRenderer(load, { budget, onSwitch })
    renderer.setVisible(false)
    await finish()
    expect(addons[0]?.disposed).toBe(true)
    expect(renderer.hasRenderer()).toBe(false)
    expect(budget.inUse()).toBe(0)
    expect(onSwitch).not.toHaveBeenCalled()
  })

  it('keeps an async renderer when the terminal is shown again before it arrives', async () => {
    const budget = createGpuBudget()
    const { load, addons, finish } = slowLoad()
    const onSwitch = vi.fn()
    const renderer = gpuRenderer(load, { budget, onSwitch })
    renderer.setVisible(false)
    renderer.setVisible(true)
    expect(load).toHaveBeenCalledTimes(1)
    await finish()
    expect(addons[0]?.disposed).toBe(false)
    expect(renderer.hasRenderer()).toBe(true)
    expect(onSwitch).toHaveBeenCalledTimes(1)
  })

  it('loads once when it is shown twice during a slow load', async () => {
    const budget = createGpuBudget()
    const { load, finish } = slowLoad()
    const renderer = gpuRenderer(load, { budget })
    renderer.setVisible(true)
    renderer.setVisible(true)
    expect(load).toHaveBeenCalledTimes(1)
    await finish()
    expect(renderer.hasRenderer()).toBe(true)
  })

  it('ignores a stale async renderer from a load that was given back', async () => {
    const budget = createGpuBudget(1)
    const slow = slowLoad()
    const first = gpuRenderer(slow.load, { budget })
    first.setVisible(false)
    const second = handle(budget)
    expect(second.addons).toHaveLength(1)
    expect(budget.inUse()).toBe(1)
    await slow.finish()
    expect(slow.addons[0]?.disposed).toBe(true)
    expect(budget.inUse()).toBe(1)
    expect(second.renderer.hasRenderer()).toBe(true)
  })

  it('drops a renderer whose context was lost, then loads at the next show (AC 34)', () => {
    const budget = createGpuBudget()
    const { addons, onSwitch, renderer } = handle(budget)
    expect(onSwitch).toHaveBeenCalledTimes(1)
    addons[0]?.lose()
    expect(addons[0]?.disposed).toBe(true)
    expect(renderer.hasRenderer()).toBe(false)
    expect(budget.inUse()).toBe(0)
    expect(budget.losses()).toBe(1)
    expect(budget.enabled()).toBe(true)
    expect(onSwitch).toHaveBeenCalledTimes(2)
    renderer.setVisible(false)
    renderer.setVisible(true)
    expect(addons).toHaveLength(2)
    expect(addons[1]?.disposed).toBe(false)
  })

  it('ignores a lost event from a renderer that is no longer current', () => {
    const budget = createGpuBudget()
    const { addons, onSwitch, renderer } = handle(budget)
    addons[0]?.lose()
    renderer.setVisible(false)
    renderer.setVisible(true)
    const switches = onSwitch.mock.calls.length
    addons[0]?.lose()
    expect(addons[1]?.disposed).toBe(false)
    expect(budget.losses()).toBe(1)
    expect(onSwitch).toHaveBeenCalledTimes(switches)
  })

  it('turns the GPU off for every terminal after two losses (AC 34)', () => {
    const budget = createGpuBudget()
    const hidden = handle(budget)
    const other = handle(budget)
    hidden.renderer.setVisible(false)
    hidden.addons[0]?.lose()
    expect(budget.enabled()).toBe(true)
    other.addons[0]?.lose()
    expect(budget.losses()).toBe(MAX_CONTEXT_LOSSES)
    expect(budget.enabled()).toBe(false)

    hidden.renderer.setVisible(true)
    other.renderer.setVisible(false)
    other.renderer.setVisible(true)
    const fresh = handle(budget)
    expect(hidden.load).toHaveBeenCalledTimes(1)
    expect(other.load).toHaveBeenCalledTimes(1)
    expect(fresh.load).not.toHaveBeenCalled()
  })

  it('lets renderers that are already held stay after the GPU is turned off', () => {
    const budget = createGpuBudget()
    const keeper = handle(budget)
    const a = handle(budget)
    const b = handle(budget)
    a.addons[0]?.lose()
    b.addons[0]?.lose()
    expect(budget.enabled()).toBe(false)
    expect(keeper.addons[0]?.disposed).toBe(false)
    expect(keeper.renderer.hasRenderer()).toBe(true)
  })

  it('calls onSwitch after a load and after a give back (AC 34)', () => {
    const budget = createGpuBudget(1)
    const first = handle(budget)
    expect(first.onSwitch).toHaveBeenCalledTimes(1)
    first.renderer.setVisible(false)
    const second = handle(budget)
    expect(first.onSwitch).toHaveBeenCalledTimes(2)
    expect(second.onSwitch).toHaveBeenCalledTimes(1)
  })

  it('starts a fresh page session on reset', () => {
    const budget = createGpuBudget()
    const a = handle(budget)
    const b = handle(budget)
    a.addons[0]?.lose()
    b.addons[0]?.lose()
    budget.reset()
    expect(budget.enabled()).toBe(true)
    expect(budget.losses()).toBe(0)
    expect(budget.inUse()).toBe(0)
  })

  it('has 6 contexts by default, one per pane of a full grid', () => {
    expect(createGpuBudget().size).toBe(6)
    expect(GPU_CONTEXTS).toBe(MAX_PANES)
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
