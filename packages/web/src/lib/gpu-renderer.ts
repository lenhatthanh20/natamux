type Renderer = { dispose(): void }

// Long enough that switching tabs away and back keeps the GPU renderer.
export const RELEASE_DELAY_MS = 10_000

// Each WebGL renderer holds a GPU context and its glyph textures, and a browser allows only a few
// contexts at once. A terminal that stays hidden gives its renderer back and falls back to the DOM
// renderer, which draws nothing while hidden. It takes a new renderer when it shows again.
// `load` returns null when WebGL cannot start, and calls `onLost` when the GPU drops the context.
export function gpuRenderer(
  load: (onLost: () => void) => Renderer | null,
  releaseDelayMs = RELEASE_DELAY_MS,
) {
  let renderer: Renderer | null = null
  let timer: ReturnType<typeof setTimeout> | undefined
  const release = () => {
    renderer?.dispose()
    renderer = null
  }
  const take = () => {
    const taken = load(() => {
      if (renderer === taken) {
        release()
      }
    })
    renderer = taken
  }
  take()

  return {
    setVisible(visible: boolean) {
      clearTimeout(timer)
      timer = undefined
      if (visible) {
        if (renderer === null) {
          take()
        }
      } else if (renderer !== null) {
        timer = setTimeout(release, releaseDelayMs)
      }
    },
    dispose() {
      clearTimeout(timer)
      timer = undefined
      release()
    },
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
