import type { TerminalSize } from '@natamux/core'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { gpuBudget } from './gpu-renderer'
import { fontStack, saveTerminalFont } from './terminal-font'
import {
  createTerminalSession,
  DISPOSE_DELAY_MS,
  FIT_DELAY_MS,
  type TerminalSession,
} from './terminal-session'
import { terminalTheme } from './terminal-theme'
import { uiZoom } from './theme'
import { type FakeTerminal, fakeSocket, fakes, resetFakes } from './xterm-fake.fixture'

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

type FakeObserver = { callback: () => void; observed: Element[]; disconnected: boolean }
let observers: FakeObserver[] = []
let frames = new Map<number, FrameRequestCallback>()
let lastFrame = 0

const runFrames = () => {
  const pending = [...frames.values()]
  frames.clear()
  for (const callback of pending) {
    callback(0)
  }
}

// Lets the promise of connect() and its then() run.
const flush = async () => {
  for (let step = 0; step < 5; step += 1) {
    await Promise.resolve()
  }
}

function make(onEnd?: (session: TerminalSession) => void) {
  const socket = fakeSocket()
  const connect = vi.fn(async (_size: TerminalSize) => socket.asWebSocket())
  const session = createTerminalSession({ connect, onEnd })
  const terminal = session.terminal as unknown as FakeTerminal
  return { session, socket, connect, terminal }
}

function box(): HTMLElement {
  const element = document.createElement('div')
  document.body.append(element)
  return element
}

// A session on screen with its socket open.
async function shown(onEnd?: (session: TerminalSession) => void) {
  const made = make(onEnd)
  const container = box()
  made.session.show(container)
  await flush()
  made.socket.open()
  return { ...made, container }
}

const fitsOf = () => fakes.fits[0]?.fits ?? 0

beforeEach(() => {
  resetFakes()
  gpuBudget.reset()
  observers = []
  frames = new Map()
  lastFrame = 0
  vi.useFakeTimers()
  vi.stubGlobal(
    'ResizeObserver',
    class {
      private readonly record: FakeObserver
      constructor(callback: () => void) {
        this.record = { callback, observed: [], disconnected: false }
        observers.push(this.record)
      }
      observe(element: Element) {
        this.record.observed.push(element)
      }
      disconnect() {
        this.record.disconnected = true
      }
    },
  )
  vi.stubGlobal('requestAnimationFrame', (callback: FrameRequestCallback) => {
    lastFrame += 1
    frames.set(lastFrame, callback)
    return lastFrame
  })
  vi.stubGlobal('cancelAnimationFrame', (id: number) => {
    frames.delete(id)
  })
})

afterEach(() => {
  // Stubs first: they were made after the fake timers, so they restore the fakes.
  vi.unstubAllGlobals()
  vi.useRealTimers()
  saveTerminalFont(null)
  document.documentElement.removeAttribute('data-theme')
  document.body.replaceChildren()
})

describe('showing a terminal session', () => {
  it('opens xterm in its own element in the container and connects with the fitted size', () => {
    const { session, connect, terminal } = make()
    terminal.fitSize = { cols: 100, rows: 30 }
    const container = box()
    session.show(container)

    const host = terminal.element?.parentElement
    expect(host?.parentElement).toBe(container)
    expect(connect).toHaveBeenCalledTimes(1)
    expect(connect).toHaveBeenCalledWith({ cols: 100, rows: 30 })
    expect(observers.at(-1)?.observed).toEqual([container])
    expect(session.state().status).toBe('connecting')
  })

  it('keeps the socket while hidden and fits nothing then', async () => {
    const { session, socket, terminal } = await shown()
    const observer = observers.at(-1)
    const holder = box()
    session.hide(holder)

    expect(holder.contains(terminal.element ?? null)).toBe(true)
    expect(socket.closed).toBe(false)
    expect(observer?.disconnected).toBe(true)

    const fits = fitsOf()
    terminal.fitSize = { cols: 20, rows: 5 }
    observer?.callback()
    vi.advanceTimersByTime(FIT_DELAY_MS * 2)
    expect(fitsOf()).toBe(fits)
    expect(socket.sent).toEqual([])
  })

  it('shows again with no new connection, one fit and a fresh draw (AC 1)', async () => {
    const { session, socket, connect, terminal } = await shown()
    const holder = box()
    session.hide(holder)
    const fits = fitsOf()
    const refreshes = terminal.refreshes

    const next = box()
    session.show(next)
    expect(next.contains(terminal.element ?? null)).toBe(true)
    expect(connect).toHaveBeenCalledTimes(1)
    expect(fitsOf()).toBe(fits + 1)
    expect(terminal.refreshes).toBe(refreshes + 1)
    // Same size as before: nothing to tell the daemon.
    expect(socket.sent).toEqual([])

    session.hide(holder)
    terminal.fitSize = { cols: 90, rows: 24 }
    session.show(box())
    expect(socket.sent).toEqual([{ type: 'resize', cols: 90, rows: 24 }])
  })

  it('fits once after the container settles on a new size', async () => {
    const { session, socket, terminal } = await shown()
    const observer = observers.at(-1)
    const fits = fitsOf()
    terminal.fitSize = { cols: 120, rows: 40 }
    observer?.callback()
    observer?.callback()
    vi.advanceTimersByTime(FIT_DELAY_MS)

    expect(fitsOf()).toBe(fits + 1)
    expect(socket.sent).toEqual([{ type: 'resize', cols: 120, rows: 40 }])
    session.dispose()
  })

  it('writes the output that arrives while hidden, in order', async () => {
    const { session, socket, terminal } = await shown()
    session.hide(box())
    for (const data of ['one ', 'two ', 'three']) {
      socket.message({ type: 'output', data })
    }
    session.show(box())
    expect(terminal.written).toEqual(['one ', 'two ', 'three'])
  })

  it('keeps every chunk of 10 minutes hidden at 2 KB/s, on the same socket (AC 2a)', async () => {
    const { session, socket, connect, terminal } = await shown()
    session.hide(box())
    const chunks: string[] = []
    for (let second = 0; second < 600; second += 1) {
      const data = `${String(second).padStart(4, '0')}${'x'.repeat(2044)}`
      chunks.push(data)
      socket.message({ type: 'output', data })
      vi.advanceTimersByTime(1000)
    }
    session.show(box())

    expect(terminal.written).toEqual(chunks)
    expect(connect).toHaveBeenCalledTimes(1)
    expect(socket.closed).toBe(false)
    expect(socket.sent).toEqual([])
  })

  it('sends what the user types', async () => {
    const { socket, terminal } = await shown()
    terminal.fire('data', 'ls\r')
    expect(socket.sent).toEqual([{ type: 'input', data: 'ls\r' }])
  })
})

describe('the end of a terminal session (AC 5)', () => {
  it('ends on an exit message and reports it once', async () => {
    const onEnd = vi.fn()
    const { session, socket, terminal } = await shown(onEnd)
    socket.message({ type: 'exit', code: 0 })
    socket.drop()

    expect(session.state().status).toBe('ended')
    expect(terminal.written.at(-1)).toBe('\r\n')
    expect(onEnd).toHaveBeenCalledTimes(1)
    expect(onEnd).toHaveBeenCalledWith(session)
  })

  it('shows the error of an error message and reports it once', async () => {
    const onEnd = vi.fn()
    const { session, socket } = await shown(onEnd)
    socket.message({ type: 'error', message: 'No shell.' })
    socket.drop()

    expect(session.state()).toMatchObject({ status: 'error', error: 'No shell.' })
    expect(onEnd).toHaveBeenCalledTimes(1)
  })

  it('treats a close before the open as a failed connection', async () => {
    const onEnd = vi.fn()
    const { session, socket } = make(onEnd)
    session.show(box())
    await flush()
    socket.drop()

    expect(session.state()).toMatchObject({
      status: 'error',
      error: 'Could not connect to the terminal.',
    })
    expect(onEnd).toHaveBeenCalledTimes(1)
  })

  it('treats a close after the open as a shell that ended', async () => {
    const onEnd = vi.fn()
    const { session, socket } = await shown(onEnd)
    expect(session.state().status).toBe('open')
    socket.drop()

    expect(session.state()).toMatchObject({ status: 'ended', error: null })
    expect(onEnd).toHaveBeenCalledTimes(1)
  })

  it('shows why connect failed', async () => {
    const onEnd = vi.fn()
    const session = createTerminalSession({
      connect: () => Promise.reject(new Error('No ticket.')),
      onEnd,
    })
    session.show(box())
    await flush()

    expect(session.state()).toMatchObject({ status: 'error', error: 'No ticket.' })
    expect(onEnd).toHaveBeenCalledTimes(1)
  })

  it('tells its subscribers about each change', async () => {
    const { session, socket } = make()
    const statuses: string[] = []
    session.subscribe(() => statuses.push(session.state().status))
    session.show(box())
    await flush()
    socket.open()
    socket.drop()
    expect(statuses).toEqual(['open', 'ended'])
  })
})

describe('disposing a terminal session (AC 3)', () => {
  it('closes the socket at once, and frees xterm and the GPU a moment later', async () => {
    const { session, socket, terminal } = await shown()
    expect(fakes.webgl).toHaveLength(1)

    session.dispose()
    expect(socket.closed).toBe(true)
    expect(terminal.element?.isConnected).toBe(false)
    expect(terminal.listenerCount('data')).toBe(0)
    expect(terminal.disposed).toBe(false)

    vi.advanceTimersByTime(DISPOSE_DELAY_MS)
    expect(terminal.disposed).toBe(true)
    expect(fakes.webgl[0]?.disposed).toBe(true)
    expect(() => session.dispose()).not.toThrow()
  })

  it('closes a socket that connects after the dispose', async () => {
    const socket = fakeSocket()
    const pending: { resolve?: (opened: WebSocket) => void } = {}
    const session = createTerminalSession({
      connect: () =>
        new Promise<WebSocket>((resolve) => {
          pending.resolve = resolve
        }),
    })
    session.show(box())
    session.dispose()

    pending.resolve?.(socket.asWebSocket())
    await flush()
    socket.open()
    expect(socket.closed).toBe(true)
    expect(session.state().status).toBe('connecting')
  })

  it('ignores a show after the dispose', async () => {
    const { session, terminal } = await shown()
    session.dispose()
    const container = box()
    session.show(container)
    expect(container.contains(terminal.element ?? null)).toBe(false)
  })
})

describe('the scroll state (spec 8)', () => {
  it('changes at most once per frame, and only when the value changes', async () => {
    const { session, terminal } = await shown()
    runFrames()
    const values: boolean[] = []
    session.subscribe(() => values.push(session.state().scrolledUp))

    terminal.buffer.active = { viewportY: 0, baseY: 10 }
    for (let count = 0; count < 50; count += 1) {
      terminal.fire('writeParsed')
    }
    expect(frames.size).toBe(1)
    runFrames()
    expect(values).toEqual([true])

    terminal.fire('scroll')
    runFrames()
    expect(values).toEqual([true])
  })

  it('does nothing while hidden and checks once when back on screen', async () => {
    const { session, terminal } = await shown()
    runFrames()
    terminal.buffer.active = { viewportY: 0, baseY: 10 }
    terminal.fire('writeParsed')
    runFrames()
    expect(session.state().scrolledUp).toBe(true)

    const values: boolean[] = []
    session.subscribe(() => values.push(session.state().scrolledUp))
    session.setOnScreen(false)
    terminal.buffer.active = { viewportY: 10, baseY: 10 }
    terminal.fire('writeParsed')
    terminal.fire('scroll')
    expect(frames.size).toBe(0)
    expect(values).toEqual([])

    session.setOnScreen(true)
    expect(frames.size).toBe(1)
    runFrames()
    expect(values).toEqual([false])
  })

  it('drops a frame that was asked for before the terminal left the screen', async () => {
    const { session, terminal } = await shown()
    runFrames()
    terminal.buffer.active = { viewportY: 0, baseY: 10 }
    terminal.fire('writeParsed')
    session.setOnScreen(false)
    expect(frames.size).toBe(0)
    expect(session.state().scrolledUp).toBe(false)
  })

  it('keeps the same state object until something changes', async () => {
    const { session } = await shown()
    const before = session.state()
    runFrames()
    expect(session.state()).toBe(before)
  })
})

describe('settings of a hidden terminal (AC 6)', () => {
  it('measures nothing while hidden, and takes new theme and font at the next show', async () => {
    document.documentElement.dataset.theme = 'light'
    const { session, terminal } = await shown()
    expect(terminal.options.theme).toEqual(terminalTheme('light'))
    session.hide(box())
    const fits = fitsOf()
    const family = terminal.options.fontFamily

    document.documentElement.dataset.theme = 'dark'
    saveTerminalFont({ family: 'Fira Code', size: 16 })
    session.applySettings()
    expect(terminal.options.fontFamily).toBe(family)
    expect(fitsOf()).toBe(fits)

    session.show(box())
    expect(terminal.options.theme).toEqual(terminalTheme('dark'))
    expect(terminal.options.fontFamily).toBe(fontStack('Fira Code'))
    expect(terminal.options.fontSize).toBe(16 * uiZoom())
    expect(fitsOf()).toBe(fits + 1)
  })

  it('applies a font change on screen at once, and fits', async () => {
    const { session, terminal } = await shown()
    const fits = fitsOf()
    saveTerminalFont({ size: 18 })
    session.applySettings()
    expect(terminal.options.fontSize).toBe(18 * uiZoom())
    expect(fitsOf()).toBe(fits + 1)

    session.applySettings()
    expect(fitsOf()).toBe(fits + 1)
  })
})

describe('visibility', () => {
  it('is visible only on screen and while the page is visible', async () => {
    const { session } = await shown()
    expect(session.isVisible()).toBe(true)

    Object.defineProperty(document, 'visibilityState', { configurable: true, get: () => 'hidden' })
    expect(session.isVisible()).toBe(false)
    Reflect.deleteProperty(document, 'visibilityState')
    expect(session.isVisible()).toBe(true)

    session.setOnScreen(false)
    expect(session.isVisible()).toBe(false)
    session.setOnScreen(true)
    session.hide(box())
    expect(session.isVisible()).toBe(false)
  })

  it('keeps its WebGL renderer while hidden, with no timer (AC 33)', async () => {
    const { session } = await shown()
    expect(fakes.webgl).toHaveLength(1)

    session.setOnScreen(false)
    vi.advanceTimersByTime(60_000)
    expect(fakes.webgl[0]?.disposed).toBe(false)

    session.setOnScreen(true)
    expect(fakes.webgl).toHaveLength(1)
  })

  it('fits again when the renderer changes', async () => {
    await shown()
    const fits = fitsOf()
    fakes.webgl[0]?.loseContext()
    expect(fitsOf()).toBe(fits + 1)
  })
})
