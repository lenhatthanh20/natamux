import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { gpuBudget } from './gpu-renderer'
import { createTerminalSession, type TerminalSession } from './terminal-session'
import { fakes, resetFakes } from './xterm-fake.fixture'

// Its own file: the import of the WebGL addon is made once per page, and here it fails.

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
// As a chunk that the network could not fetch.
vi.mock('@xterm/addon-webgl', () => {
  throw new Error('Failed to fetch dynamically imported module')
})

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

describe('a WebGL addon that fails to load', () => {
  it('leaves the terminal drawing with the DOM, and frees its GPU slot', async () => {
    const session = open()
    expect(gpuBudget.inUse()).toBe(1)

    await vi.waitFor(() => expect(gpuBudget.inUse()).toBe(0))
    expect(fakes.webgl).toHaveLength(0)

    // Shown again, it asks again and gets the same answer.
    session.hide(document.createElement('div'))
    session.show(document.body)
    await vi.waitFor(() => expect(gpuBudget.inUse()).toBe(0))
    expect(fakes.webgl).toHaveLength(0)
    expect(fakes.terminals[0]?.element?.isConnected).toBe(true)
  })
})
