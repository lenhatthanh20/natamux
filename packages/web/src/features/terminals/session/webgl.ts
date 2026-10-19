import type { WebglAddon } from '@xterm/addon-webgl'
import type { Terminal } from '@xterm/xterm'
import type { Renderer } from '../gpu-renderer'

// The WebGL addon loads with the first GPU renderer, so a page that never gets one never fetches
// it. Once loaded, a renderer starts in the same call: a terminal shown again draws with the GPU
// in its first frame.
let webglImport: Promise<typeof WebglAddon> | undefined
let webglClass: typeof WebglAddon | undefined

export function importWebgl(): Promise<typeof WebglAddon> {
  webglImport ??= import('@xterm/addon-webgl').then((module) => {
    webglClass = module.WebglAddon
    return module.WebglAddon
  })
  return webglImport
}

// Draws with the GPU, as native terminals do. Without WebGL (old GPU, tests) or when the GPU
// drops the context, the terminal keeps or goes back to drawing with the DOM. A failed import is
// the same as no WebGL (gpuRenderer).
export function loadWebgl(
  terminal: Terminal,
  onLost: () => void,
): Renderer | null | Promise<Renderer | null> {
  const Webgl = webglClass
  if (Webgl === undefined) {
    return importWebgl().then(() => loadWebgl(terminal, onLost))
  }
  try {
    const webgl = new Webgl()
    webgl.onContextLoss(onLost)
    // The addon adds two canvases: a 2D one for links, then its WebGL one. Asking any other canvas
    // for a context would make one.
    const before = new Set(terminal.element?.querySelectorAll('canvas'))
    terminal.loadAddon(webgl)
    const after = terminal.element?.querySelectorAll('canvas') ?? []
    const canvas = [...after].find(
      (one) => !before.has(one) && !one.classList.contains('xterm-link-layer'),
    )
    return {
      dispose() {
        webgl.dispose()
        // xterm drops the canvas but keeps its context, which holds GPU memory until garbage
        // collection and counts toward the browser's limit of contexts. The addon's loss
        // listener went with dispose, so this loss never reaches onLost.
        const gl = canvas?.getContext('webgl2')
        if (gl && !gl.isContextLost()) {
          gl.getExtension('WEBGL_lose_context')?.loseContext()
        }
      },
    }
  } catch {
    return null
  }
}
