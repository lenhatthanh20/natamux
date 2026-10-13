export type Renderer = { dispose(): void }

// A browser allows only a few WebGL contexts at once, so one page session shares this many. It
// matches MAX_PANES (lib/panes.ts): a full grid of panes all draw with the GPU.
export const GPU_CONTEXTS = 6
// After this many lost contexts the GPU is not trusted for the rest of the page session.
export const MAX_CONTEXT_LOSSES = 2
// A terminal hidden this long gives its renderer back. A quick switch back keeps it.
export const GIVE_BACK_DELAY_MS = 10_000

// The WebGL renderers of one page session. Tests make their own.
export type GpuBudget = {
  readonly size: number
  inUse(): number // renderers held now, loading ones included
  losses(): number
  enabled(): boolean // false after MAX_CONTEXT_LOSSES losses
  reset(): void // tests only: a fresh page session
}

// A handle as the budget sees it: enough to pick who gives a renderer back.
type Slot = {
  isHidden(): boolean
  hiddenAt(): number
  giveBack(): void
}

type BudgetInternals = {
  slots: Set<Slot>
  recordLoss(): void
}

// Kept apart from the public type so callers cannot reach into the slots.
const internals = new WeakMap<GpuBudget, BudgetInternals>()

export function createGpuBudget(size = GPU_CONTEXTS): GpuBudget {
  const slots = new Set<Slot>()
  let losses = 0
  const budget: GpuBudget = {
    size,
    inUse: () => slots.size,
    losses: () => losses,
    enabled: () => losses < MAX_CONTEXT_LOSSES,
    reset() {
      slots.clear()
      losses = 0
    },
  }
  internals.set(budget, {
    slots,
    recordLoss() {
      losses += 1
    },
  })
  return budget
}

export const gpuBudget: GpuBudget = createGpuBudget()

export type GpuRenderer = {
  setVisible(visible: boolean): void
  dispose(): void
  hasRenderer(): boolean
}

type LoadResult = Renderer | null | Promise<Renderer | null>

function isThenable(value: LoadResult): value is Promise<Renderer | null> {
  return value !== null && typeof (value as { then?: unknown }).then === 'function'
}

// Counts hides across all handles, so "hidden longest" needs no clock.
let hiddenClock = 0

// Each WebGL renderer holds a GPU context and its glyph textures (tens of MB at Retina). A hidden
// terminal keeps its renderer for GIVE_BACK_DELAY_MS, so a quick switch back is instant, then
// gives it back and falls back to the DOM renderer; it takes one again when shown. Before that
// delay, a full budget makes the terminal that has been hidden longest give its renderer back.
// `load` returns null when WebGL cannot start, and calls `onLost` when the GPU drops the context.
// `onSwitch` runs after the renderer type changed, so the caller can fit again.
export function gpuRenderer(
  load: (onLost: () => void) => LoadResult,
  options: { onSwitch?: () => void; budget?: GpuBudget } = {},
): GpuRenderer {
  const budget = options.budget ?? gpuBudget
  const shared = internals.get(budget)
  if (shared === undefined) {
    throw new Error('Unknown GPU budget')
  }
  const onSwitch = options.onSwitch ?? (() => undefined)

  let renderer: Renderer | null = null
  let loading = false
  let disposed = false
  let visible = true
  let hiddenAt = 0
  let loadId = 0
  let giveBackTimer: ReturnType<typeof setTimeout> | undefined

  const slot: Slot = {
    isHidden: () => !visible,
    hiddenAt: () => hiddenAt,
    giveBack() {
      // Dropping the id also drops a load that is still running.
      loadId += 1
      loading = false
      shared.slots.delete(slot)
      if (renderer !== null) {
        renderer.dispose()
        renderer = null
        onSwitch()
      }
    },
  }

  const free = () => {
    loading = false
    shared.slots.delete(slot)
  }

  const settle = (id: number, result: Renderer | null) => {
    if (id !== loadId) {
      // A newer take or a dispose owns the slot now. Only this renderer is left to clean up.
      result?.dispose()
      return
    }
    if (result === null) {
      free()
      return
    }
    if (disposed || !visible) {
      result.dispose()
      free()
      return
    }
    renderer = result
    loading = false
    onSwitch()
  }

  const take = () => {
    if (disposed || renderer !== null || loading || !budget.enabled()) {
      return
    }
    if (shared.slots.size >= budget.size) {
      let oldest: Slot | undefined
      for (const other of shared.slots) {
        if (other.isHidden() && (oldest === undefined || other.hiddenAt() < oldest.hiddenAt())) {
          oldest = other
        }
      }
      if (oldest === undefined) {
        return
      }
      oldest.giveBack()
    }
    // Reserve the slot before load runs, so a second take cannot pass the budget.
    shared.slots.add(slot)
    loading = true
    loadId += 1
    const id = loadId
    let taken: Renderer | null = null
    const onLost = () => {
      if (taken === null || renderer !== taken) {
        return
      }
      taken.dispose()
      renderer = null
      shared.slots.delete(slot)
      shared.recordLoss()
      onSwitch()
    }
    const wrap = (result: Renderer | null) => {
      taken = result
      settle(id, result)
    }
    const result = load(onLost)
    if (isThenable(result)) {
      result.then(wrap, () => settle(id, null))
    } else {
      wrap(result)
    }
  }

  take()

  return {
    setVisible(next: boolean) {
      if (disposed) {
        return
      }
      if (next) {
        visible = true
        clearTimeout(giveBackTimer)
        take()
      } else if (visible) {
        visible = false
        hiddenClock += 1
        hiddenAt = hiddenClock
        giveBackTimer = setTimeout(slot.giveBack, GIVE_BACK_DELAY_MS)
      }
    },
    dispose() {
      if (disposed) {
        return
      }
      disposed = true
      clearTimeout(giveBackTimer)
      loadId += 1
      free()
      renderer?.dispose()
      renderer = null
    },
    hasRenderer: () => renderer !== null,
  }
}

// Calls onChange when the element starts or stops being drawn, such as when a parent gets
// `hidden`. Returns a function that stops watching.
export function watchVisible(element: Element, onChange: (visible: boolean) => void): () => void {
  if (typeof IntersectionObserver === 'undefined') {
    return () => undefined
  }
  const observer = new IntersectionObserver((entries) => {
    const last = entries[entries.length - 1]
    if (last !== undefined) {
      onChange(last.isIntersecting)
    }
  })
  observer.observe(element)
  return () => observer.disconnect()
}
