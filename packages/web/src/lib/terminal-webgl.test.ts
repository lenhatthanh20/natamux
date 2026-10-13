import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { gpuBudget } from './gpu-renderer'
import { createTerminalSession, type TerminalSession } from './terminal-session'
import { fakes, resetFakes } from './xterm-fake.fixture'

// Its own file: the WebGL addon is not loaded yet when the first test runs, as on a fresh page.

vi.mock('@xterm/xterm', async () => ({
  Terminal: (await import('./xterm-fake.fixture')).FakeTerminal,
}))
vi.mock('@xterm/addon-fit', async () => ({
  FitAddon: (await import('./xterm-fake.fixture')).FakeFitAddon,
}))
vi.mock('@xterm/addon-search', async () => ({
  SearchAddon: (await import('./xterm-fake.fixture')).FakeSearchAddon,
}))
vi.mock('@xterm/addon-unicode11', async () => ({
  Unicode11Addon: (await import('./xterm-fake.fixture')).FakeAddon,
}))
vi.mock('@xterm/addon-web-links', async () => ({
  WebLinksAddon: (await import('./xterm-fake.fixture')).FakeAddon,
}))
vi.mock('@xterm/addon-webgl', async () => ({
  WebglAddon: (await import('./xterm-fake.fixture')).FakeWebglAddon,
}))

const sessions: TerminalSession[] = []

// A session on screen. The socket never opens: only the renderer matters here.
function open(): TerminalSession {
  const session = createTerminalSession({ connect: () => new Promise<WebSocket>(() => undefined) })
  const container = document.createElement('div')
  document.body.append(container)
  session.show(container)
  sessions.push(session)
  return session
}

beforeEach(() => {
  resetFakes()
  gpuBudget.reset()
  vi.stubGlobal(
    'ResizeObserver',
    class {
      observe = () => undefined
      disconnect = () => undefined
    },
  )
})

afterEach(() => {
  for (const session of sessions.splice(0)) {
    session.dispose()
  }
  vi.unstubAllGlobals()
  document.body.replaceChildren()
})

describe('the WebGL addon', () => {
  it('loads with the first GPU renderer, and the next renderers start at once', async () => {
    open()
    // The DOM draws until the addon arrives.
    expect(fakes.webgl).toHaveLength(0)
    expect(gpuBudget.inUse()).toBe(1)
    const fits = fakes.fits[0]?.fits ?? 0

    await vi.waitFor(() => expect(fakes.webgl).toHaveLength(1))
    // The renderer can change the cell size, so the terminal fits again.
    expect(fakes.fits[0]?.fits).toBe(fits + 1)

    open()
    expect(fakes.webgl).toHaveLength(2)
  })
})
