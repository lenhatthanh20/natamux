import {
  ACK_EVERY_BYTES,
  CLOSE_GONE,
  CLOSE_REFUSED,
  CLOSE_UNAUTHORIZED,
  HEARTBEAT_MS,
  type TerminalSize,
} from '@natamux/core'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ICON_FONT } from './fonts'
import { GPU_CONTEXTS, gpuBudget } from './gpu-renderer'
import type { TerminalConnectQuery } from './terminal'
import { fontStack, saveTerminalFont, terminalFont } from './terminal-font'
import {
  CATCH_UP_TIMEOUT_MS,
  createTerminalSession,
  DISPOSE_DELAY_MS,
  FIT_DELAY_MS,
  HIDDEN_RESUME_BYTES,
  type TerminalSession,
} from './terminal-session'
import { TERMINAL_LOOK, terminalTheme } from './terminal-theme'
import { uiZoom } from './theme'
import { applyUiSize, uiSizeZoom } from './ui-size'
import {
  type FakeSocket,
  type FakeTerminal,
  fakeSocket,
  fakes,
  resetFakes,
} from './xterm-fake.fixture'

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
  const connect = vi.fn(async (_query: TerminalConnectQuery) => socket.asWebSocket())
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

// The daemon's side of an attach: hello, snapshot and its chunks. `replayed` comes only with an
// offset, so a test can hold it back.
function snapshotOf(socket: FakeSocket, size: TerminalSize, chunks: string[], offset?: number) {
  socket.message({ type: 'hello', shellId: 'shell-1' })
  socket.message({ type: 'snapshot', ...size })
  for (const chunk of chunks) {
    socket.bytes(chunk)
  }
  if (offset !== undefined) {
    socket.message({ type: 'replayed', offset })
  }
}

// jsdom's page is always visible; this sets it and tells the page, as a browser does.
function setPageHidden(hidden: boolean) {
  Object.defineProperty(document, 'visibilityState', {
    value: hidden ? 'hidden' : 'visible',
    configurable: true,
  })
  document.dispatchEvent(new Event('visibilitychange'))
}

// Records every option the code sets from now on, in order.
function recordOptions(terminal: FakeTerminal): [string, unknown][] {
  const sets: [string, unknown][] = []
  terminal.options = new Proxy(terminal.options, {
    set(target, key, value) {
      sets.push([String(key), value])
      return Reflect.set(target, key, value)
    },
  })
  return sets
}

// jsdom has no document.fonts. Each load waits until the test resolves it.
function stubFontLoads(): (() => void)[] {
  const loads: (() => void)[] = []
  const load = () =>
    new Promise<void>((resolve) => {
      loads.push(resolve)
    })
  const fonts = Object.assign(new EventTarget(), { load })
  Object.defineProperty(document, 'fonts', { value: fonts, configurable: true })
  return loads
}

// What the browser fires when font faces finished loading.
function fontsLoaded(...families: string[]): void {
  const event = Object.assign(new Event('loadingdone'), {
    fontfaces: families.map((family) => ({ family })),
  })
  document.fonts.dispatchEvent(event)
}

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
  vi.restoreAllMocks()
  vi.useRealTimers()
  saveTerminalFont(null)
  document.documentElement.removeAttribute('data-theme')
  document.documentElement.style.removeProperty('--ui-zoom')
  window.localStorage.clear()
  Reflect.deleteProperty(document, 'fonts')
  Reflect.deleteProperty(document, 'visibilityState')
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
    expect(connect).toHaveBeenCalledWith({ cols: 100, rows: 30, visible: true })
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
    expect(socket.sent).toEqual([{ type: 'visible', visible: false }])
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
    // Same size as before: no resize, only that it is on screen again with its size.
    expect(socket.sent).toEqual([
      { type: 'visible', visible: false },
      { type: 'visible', visible: true, cols: 80, rows: 24 },
    ])

    session.hide(holder)
    socket.sent.length = 0
    terminal.fitSize = { cols: 90, rows: 24 }
    session.show(box())
    expect(socket.sent).toEqual([
      { type: 'resize', cols: 90, rows: 24 },
      { type: 'visible', visible: true, cols: 90, rows: 24 },
    ])
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
      socket.bytes(data)
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
      socket.bytes(data)
      vi.advanceTimersByTime(1000)
    }
    session.show(box())

    expect(terminal.written).toEqual(chunks)
    expect(connect).toHaveBeenCalledTimes(1)
    expect(socket.closed).toBe(false)
    expect(socket.sent).toEqual([
      { type: 'visible', visible: false },
      { type: 'visible', visible: true, cols: 80, rows: 24 },
    ])
  })

  it('takes the focus when shown again while another control has it (C1)', async () => {
    const { session, terminal } = await shown()
    session.hide(box())
    const button = document.createElement('button')
    document.body.append(button)
    button.focus()
    expect(document.activeElement).toBe(button)
    const focuses = terminal.focuses

    session.show(box())
    expect(terminal.focuses).toBe(focuses + 1)
  })

  it('leaves the focus on the tab that shows it again (C1)', async () => {
    const { session, terminal } = await shown()
    session.hide(box())
    const tab = document.createElement('button')
    tab.setAttribute('role', 'tab')
    document.body.append(tab)
    tab.focus()
    expect(document.activeElement).toBe(tab)
    const focuses = terminal.focuses

    session.show(box())
    expect(terminal.focuses).toBe(focuses)
  })

  it('leaves the focus where it is when shown with takeFocus false', async () => {
    const { session, terminal } = await shown()
    session.hide(box())
    const button = document.createElement('button')
    document.body.append(button)
    button.focus()
    const focuses = terminal.focuses

    session.show(box(), false)
    expect(terminal.focuses).toBe(focuses)
    expect(document.activeElement).toBe(button)
  })

  it('fits once and sends one resize after a drag of 60 steps (C38)', async () => {
    const { socket, terminal } = await shown()
    const observer = observers.at(-1)
    const fits = fitsOf()
    for (let step = 1; step <= 60; step += 1) {
      terminal.fitSize = { cols: 80 + step, rows: 24 }
      observer?.callback()
      vi.advanceTimersByTime(16)
    }
    vi.advanceTimersByTime(FIT_DELAY_MS)

    expect(fitsOf()).toBe(fits + 1)
    expect(socket.sent).toEqual([{ type: 'resize', cols: 140, rows: 24 }])
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
    // A new line after the last output, then the cursor hides: the screen is read-only now.
    expect(terminal.written.slice(-2)).toEqual(['\r\n', '\x1b[?25l'])
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

  // AC 29: the daemon closes a resume with CLOSE_GONE when the shell no longer runs.
  it('treats a close with CLOSE_GONE after the open as a shell that ended', async () => {
    const onEnd = vi.fn()
    const { session, socket } = await shown(onEnd)
    expect(session.state().status).toBe('open')
    socket.drop(CLOSE_GONE)

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
    expect(statuses).toEqual(['open', 'reconnecting'])
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

  it('applies a new UI size on screen at once, with one fit (C7)', async () => {
    const { session, terminal } = await shown()
    const fits = fitsOf()
    applyUiSize('large')
    expect(uiZoom()).toBe(uiSizeZoom('large'))

    session.applySettings()
    expect(terminal.options.fontSize).toBe(terminalFont().size * uiZoom())
    expect(fitsOf()).toBe(fits + 1)
  })

  it('takes a UI size changed while hidden at the next show, with one fit (C7)', async () => {
    const { session, terminal } = await shown()
    session.hide(box())
    const fits = fitsOf()
    const size = terminal.options.fontSize

    applyUiSize('large')
    session.applySettings()
    expect(terminal.options.fontSize).toBe(size)
    expect(fitsOf()).toBe(fits)

    session.show(box())
    expect(terminal.options.fontSize).toBe(terminalFont().size * uiSizeZoom('large'))
    expect(fitsOf()).toBe(fits + 1)
  })

  it('takes only the last of several changes made while hidden, with one fit (C7)', async () => {
    document.documentElement.dataset.theme = 'light'
    const { session, terminal } = await shown()
    session.hide(box())
    const fits = fitsOf()
    const sets = recordOptions(terminal)

    document.documentElement.dataset.theme = 'dark'
    session.applySettings()
    saveTerminalFont({ family: 'Fira Code', size: 15 })
    session.applySettings()
    document.documentElement.dataset.theme = 'light'
    session.applySettings()
    saveTerminalFont({ family: 'JetBrains Mono', size: 17 })
    session.applySettings()
    // The theme needs no measuring, so it changes at once. The font waits for the show.
    expect(sets).toEqual([
      ['theme', terminalTheme('dark')],
      ['theme', terminalTheme('light')],
    ])
    expect(fitsOf()).toBe(fits)

    sets.length = 0
    session.show(box())
    expect(terminal.options.theme).toEqual(terminalTheme('light'))
    expect(terminal.options.fontFamily).toBe(fontStack('JetBrains Mono'))
    expect(terminal.options.fontSize).toBe(17 * uiZoom())
    expect(sets.filter(([key]) => key === 'fontSize')).toEqual([['fontSize', 17 * uiZoom()]])
    expect(sets.some(([key]) => key === 'theme')).toBe(false)
    expect(fitsOf()).toBe(fits + 1)
  })
})

describe('a web font that loads after the terminal opened (C7)', () => {
  it('measures again and fits when it loads on screen', async () => {
    const loads = stubFontLoads()
    const { terminal } = await shown()
    expect(loads).toHaveLength(1)
    const fits = fitsOf()
    const sets = recordOptions(terminal)

    loads[0]?.()
    await flush()
    expect(sets).toEqual([
      ['fontFamily', 'monospace'],
      ['fontFamily', terminalFont().stack],
    ])
    expect(fitsOf()).toBe(fits + 1)
  })

  it('waits for the next show when it loads while hidden', async () => {
    const loads = stubFontLoads()
    const { session, terminal } = await shown()
    session.hide(box())
    const fits = fitsOf()
    const sets = recordOptions(terminal)

    loads[0]?.()
    await flush()
    expect(sets).toEqual([])
    expect(fitsOf()).toBe(fits)

    session.show(box())
    expect(sets).toEqual([
      ['fontFamily', 'monospace'],
      ['fontFamily', terminalFont().stack],
      ['fontSize', terminalFont().size * uiZoom()],
    ])
    expect(fitsOf()).toBe(fits + 1)
  })

  it('does nothing when it loads after the dispose', async () => {
    const loads = stubFontLoads()
    const { session, terminal } = await shown()
    session.dispose()
    const fits = fitsOf()
    const sets = recordOptions(terminal)

    loads[0]?.()
    await flush()
    expect(sets).toEqual([])
    expect(fitsOf()).toBe(fits)
  })
})

describe('the look of a terminal (spec 1, 3, 4, 8)', () => {
  it('opens xterm with the shared look', () => {
    const { terminal } = make()
    expect(terminal.options).toMatchObject(TERMINAL_LOOK)
  })
})

describe('the icon font, which loads on the first icon (spec 2)', () => {
  it('draws the glyphs again when it loads, so icons replace empty boxes', async () => {
    stubFontLoads()
    const { terminal } = await shown()

    fontsLoaded('"JetBrains Mono Variable"')
    expect(terminal.atlasClears).toBe(0)

    fontsLoaded(`"${ICON_FONT}"`)
    expect(terminal.atlasClears).toBe(1)
  })

  it('stops listening after the dispose', async () => {
    stubFontLoads()
    const { session, terminal } = await shown()
    session.dispose()

    fontsLoaded(`"${ICON_FONT}"`)
    expect(terminal.atlasClears).toBe(0)
  })
})

describe('visibility', () => {
  it('keeps its WebGL renderer while hidden, with no timer (AC 33)', async () => {
    const { session } = await shown()
    expect(fakes.webgl).toHaveLength(1)

    session.setOnScreen(false)
    vi.advanceTimersByTime(60_000)
    expect(fakes.webgl[0]?.disposed).toBe(false)

    session.setOnScreen(true)
    expect(fakes.webgl).toHaveLength(1)
  })

  it('fits a hidden terminal that gave its renderer back at its next show (C36)', async () => {
    const { session } = await shown()
    session.hide(box())
    const fits = fitsOf()
    // The others fill the budget, and the last one takes the renderer of the hidden terminal.
    for (let count = 0; count < GPU_CONTEXTS; count += 1) {
      await shown()
    }
    expect(fakes.webgl[0]?.disposed).toBe(true)
    expect(fitsOf()).toBe(fits)

    session.show(box())
    expect(fitsOf()).toBe(fits + 1)
  })

  it('fits again when the renderer changes', async () => {
    await shown()
    const fits = fitsOf()
    fakes.webgl[0]?.loseContext()
    expect(fitsOf()).toBe(fits + 1)
  })
})

describe('binary output and JSON control (AC 14, C16)', () => {
  it('writes a binary frame to xterm as bytes, with no JSON step', async () => {
    const { session, socket, terminal } = await shown()
    expect(socket.binaryType).toBe('arraybuffer')
    const parse = vi.spyOn(JSON, 'parse')
    const text = 'héllo {"type":"exit","code":0}'

    socket.bytes(text)
    expect(parse).not.toHaveBeenCalled()
    const written = terminal.writes.at(-1)
    expect(typeof written).not.toBe('string')
    expect(ArrayBuffer.isView(written)).toBe(true)
    expect(terminal.written.at(-1)).toBe(text)
    expect(session.state().status).toBe('open')
  })

  it('ignores hello, an old output message and an unknown type', async () => {
    const { session, socket, terminal } = await shown()
    socket.message({ type: 'hello', shellId: 'shell-1' })
    socket.message({ type: 'output', data: 'old' })
    socket.message({ type: 'pong' })

    expect(terminal.written).toEqual([])
    expect(session.state()).toMatchObject({ status: 'open', catchingUp: false })
    expect(socket.sent).toEqual([])
  })
})

describe('connecting (spec 2.1)', () => {
  it('connects as visible, and as not visible when the page is hidden', () => {
    const first = make()
    first.session.show(box())
    expect(first.connect).toHaveBeenCalledWith({ cols: 80, rows: 24, visible: true })

    setPageHidden(true)
    const second = make()
    second.session.show(box())
    expect(second.connect).toHaveBeenCalledWith({ cols: 80, rows: 24, visible: false })
  })
})

describe('a snapshot catch-up (AC 9, C10)', () => {
  it('hides the screen, writes the snapshot at its size, and shows it at the bottom', async () => {
    const { session, socket, terminal } = await shown()
    const host = terminal.element?.parentElement
    socket.message({ type: 'hello', shellId: 'shell-1' })
    socket.message({ type: 'snapshot', cols: 100, rows: 30 })
    expect(session.state().catchingUp).toBe(true)
    expect(host?.style.visibility).toBe('hidden')
    // The textarea keeps the focus, so keys typed now are kept.
    expect(terminal.textarea?.style.visibility).toBe('visible')
    // The reset waits for the bytes written before it.
    expect(terminal.resets).toBe(0)

    terminal.parse()
    expect(terminal.resets).toBe(1)
    expect([terminal.cols, terminal.rows]).toEqual([100, 30])
    socket.bytes('history\r\n')
    socket.bytes('screen')
    terminal.buffer.active = { viewportY: 0, baseY: 10 }
    socket.message({ type: 'replayed', offset: 500 })
    // Still hidden: xterm has not parsed the last chunk yet.
    expect(session.state().catchingUp).toBe(true)
    expect(socket.sent).toEqual([])

    terminal.parse()
    expect(terminal.written).toEqual(['', 'history\r\n', 'screen', ''])
    expect([terminal.cols, terminal.rows]).toEqual([80, 24])
    expect(terminal.buffer.active.viewportY).toBe(10)
    expect(session.state().catchingUp).toBe(false)
    expect(host?.style.visibility).toBe('')
    expect(terminal.textarea?.style.visibility).toBe('')
    // Fitted back to the size it attached with: no resize.
    expect(socket.sent).toEqual([{ type: 'ack', offset: 500 }])
  })

  it('draws a small resume in place and hides a large one until its end (C10)', async () => {
    const { session, socket, terminal } = await shown()
    terminal.buffer.active = { viewportY: 0, baseY: 10 }
    socket.message({ type: 'resume', offset: 0 })
    socket.bytes('x'.repeat(HIDDEN_RESUME_BYTES))
    expect(session.state().catchingUp).toBe(false)
    socket.message({ type: 'replayed', offset: HIDDEN_RESUME_BYTES })
    terminal.parse()
    expect(terminal.resets).toBe(0)
    expect(terminal.buffer.active.viewportY).toBe(0)

    socket.message({ type: 'resume', offset: HIDDEN_RESUME_BYTES })
    socket.bytes('x'.repeat(HIDDEN_RESUME_BYTES))
    expect(session.state().catchingUp).toBe(false)
    socket.bytes('y')
    expect(session.state().catchingUp).toBe(true)
    expect(terminal.element?.parentElement?.style.visibility).toBe('hidden')
    socket.message({ type: 'replayed', offset: 2 * HIDDEN_RESUME_BYTES + 1 })
    terminal.parse()
    expect(session.state().catchingUp).toBe(false)
    expect(terminal.element?.parentElement?.style.visibility).toBe('')
    expect(terminal.resets).toBe(0)
    expect(terminal.buffer.active.viewportY).toBe(0)
  })
})

// Typed text, Vietnamese input (a, DEL, á), a paste that looks like a reply, a mouse report.
const KEYS = ['l', 'a', '\x7f', 'á', 'abc\x1b[1;2R', '\x1b[<0;3;4M']

const CATCH_UPS: [string, (socket: FakeSocket) => void][] = [
  ['a snapshot', (socket) => snapshotOf(socket, { cols: 80, rows: 24 }, ['screen'])],
  [
    'a small resume',
    (socket) => {
      socket.message({ type: 'resume', offset: 10 })
      socket.bytes('more')
    },
  ],
  [
    'a large resume',
    (socket) => {
      socket.message({ type: 'resume', offset: 10 })
      socket.bytes('x'.repeat(HIDDEN_RESUME_BYTES + 1))
    },
  ],
]

const REPLIES = [
  '\x1b[?1;2c',
  '\x1b[>0;276;0c',
  '\x1b[12;5R',
  '\x1b[0n',
  '\x1b[?2026;2$y',
  '\x1b[8;24;80t',
  '\x1bP1$r0m\x1b\\',
  '\x1b]11;rgb:0000/0000/0000\x1b\\',
  '\x1b]10;rgb:ffff/ffff/ffff\x07',
]

describe('input and size during a catch-up (AC 12, C14)', () => {
  it.each(CATCH_UPS)('keeps input and size until the end of %s', async (_, begin) => {
    const { socket, terminal } = await shown()
    const observer = observers.at(-1)
    begin(socket)
    terminal.parse()
    for (const data of KEYS) {
      terminal.fire('data', data)
    }
    terminal.fitSize = { cols: 120, rows: 40 }
    observer?.callback()
    vi.advanceTimersByTime(FIT_DELAY_MS)
    terminal.textarea?.dispatchEvent(new Event('focus'))
    expect(socket.sent).toEqual([])

    socket.message({ type: 'replayed', offset: 100 })
    terminal.parse()
    expect(socket.sent).toEqual([
      { type: 'ack', offset: 100 },
      { type: 'resize', cols: 120, rows: 40 },
      ...KEYS.map((data) => ({ type: 'input', data })),
    ])
  })

  it('never sends the replies xterm gives to replayed queries', async () => {
    const { socket, terminal } = await shown()
    socket.message({ type: 'resume', offset: 0 })
    socket.bytes('\x1b[c\x1b[6n')
    for (const reply of REPLIES) {
      terminal.fire('data', reply)
    }
    socket.message({ type: 'replayed', offset: 8 })
    terminal.parse()

    expect(socket.sent).toEqual([{ type: 'ack', offset: 8 }])
  })

  it('keeps a reply form typed with a key, and drops a reply right after it', async () => {
    const { socket, terminal } = await shown()
    snapshotOf(socket, { cols: 80, rows: 24 }, ['screen'])
    // Shift+F3 gives the same bytes as a cursor position report.
    terminal.keyData = '\x1b[1;2R'
    terminal.textarea?.dispatchEvent(new KeyboardEvent('keydown', { bubbles: true }))
    terminal.fire('data', '\x1b[1;2R')
    socket.message({ type: 'replayed', offset: 6 })
    terminal.parse()

    expect(socket.sent).toEqual([
      { type: 'ack', offset: 6 },
      { type: 'input', data: '\x1b[1;2R' },
    ])
  })

  it('keeps the key flag past a microtask and clears it with a task', async () => {
    const { socket, terminal } = await shown()
    socket.message({ type: 'resume', offset: 0 })
    // A keydown that never reaches the textarea listener, as if xterm stopped it.
    terminal.element?.dispatchEvent(new KeyboardEvent('keydown', { bubbles: true }))
    terminal.fire('data', '\x1b[2;3R')
    await Promise.resolve()
    terminal.fire('data', '\x1b[3;4R')
    vi.advanceTimersByTime(0)
    terminal.fire('data', '\x1b[4;5R')
    socket.message({ type: 'replayed', offset: 0 })
    terminal.parse()

    expect(socket.sent).toEqual([
      { type: 'ack', offset: 0 },
      { type: 'input', data: '\x1b[2;3R' },
      { type: 'input', data: '\x1b[3;4R' },
    ])
  })

  it('sends one resize with the final size after a drag during a catch-up (C38)', async () => {
    const { socket, terminal } = await shown()
    const observer = observers.at(-1)
    snapshotOf(socket, { cols: 80, rows: 24 }, ['screen'])
    terminal.parse()
    for (let step = 1; step <= 60; step += 1) {
      terminal.fitSize = { cols: 80 + step, rows: 24 }
      observer?.callback()
      vi.advanceTimersByTime(16)
    }
    vi.advanceTimersByTime(FIT_DELAY_MS)
    expect(socket.sent).toEqual([])

    socket.message({ type: 'replayed', offset: 6 })
    terminal.parse()
    expect(socket.sent).toEqual([
      { type: 'ack', offset: 6 },
      { type: 'resize', cols: 140, rows: 24 },
    ])
  })
})

describe('a catch-up whose replayed does not come (AC 13, C15)', () => {
  it('shows the screen after 10 s, sends the kept keys, then input at once', async () => {
    const { session, socket, terminal } = await shown()
    snapshotOf(socket, { cols: 80, rows: 24 }, ['screen'])
    terminal.parse()
    terminal.fire('data', 'a')
    terminal.fire('data', 'b')
    vi.advanceTimersByTime(CATCH_UP_TIMEOUT_MS - 1)
    expect(session.state().catchingUp).toBe(true)
    expect(socket.sent).toEqual([])

    vi.advanceTimersByTime(1)
    expect(session.state().catchingUp).toBe(false)
    expect(terminal.element?.parentElement?.style.visibility).toBe('')
    expect(socket.sent).toEqual([
      { type: 'input', data: 'a' },
      { type: 'input', data: 'b' },
    ])
    terminal.fire('data', 'c')
    expect(socket.sent.at(-1)).toEqual({ type: 'input', data: 'c' })
  })

  it('ends normally on replayed at 9.9 s, with no second send at 10 s', async () => {
    const { session, socket, terminal } = await shown()
    snapshotOf(socket, { cols: 80, rows: 24 }, ['screen'])
    terminal.fire('data', 'a')
    vi.advanceTimersByTime(9_900)
    socket.message({ type: 'replayed', offset: 6 })
    terminal.parse()
    expect(session.state().catchingUp).toBe(false)
    expect(socket.sent).toEqual([
      { type: 'ack', offset: 6 },
      { type: 'input', data: 'a' },
    ])

    vi.advanceTimersByTime(CATCH_UP_TIMEOUT_MS)
    expect(socket.sent).toHaveLength(2)
  })

  it('only takes the offset of a replayed after 10 s: no reset, no second show', async () => {
    const { session, socket, terminal } = await shown()
    snapshotOf(socket, { cols: 80, rows: 24 }, ['screen'])
    terminal.parse()
    vi.advanceTimersByTime(CATCH_UP_TIMEOUT_MS)
    expect(session.state().catchingUp).toBe(false)
    const changes = vi.fn()
    session.subscribe(changes)
    terminal.buffer.active = { viewportY: 0, baseY: 10 }

    socket.bytes('late')
    socket.message({ type: 'replayed', offset: 200 })
    terminal.parse()
    expect(terminal.resets).toBe(1)
    expect(terminal.buffer.active.viewportY).toBe(0)
    expect(changes).not.toHaveBeenCalled()
    expect(socket.sent).toEqual([{ type: 'ack', offset: 200 }])
  })
})

describe('the offset and acks (AC 44, C46)', () => {
  it('takes the offset from replayed, then acks every ACK_EVERY_BYTES parsed', async () => {
    const { socket, terminal } = await shown()
    // Snapshot and waiting output past the ack size: no ack during the catch-up.
    snapshotOf(socket, { cols: 80, rows: 24 }, ['x'.repeat(ACK_EVERY_BYTES), 'y'.repeat(100)])
    terminal.parse()
    expect(socket.sent).toEqual([])
    socket.message({ type: 'replayed', offset: 100 })
    terminal.parse()
    expect(socket.sent).toEqual([{ type: 'ack', offset: 100 }])

    socket.bytes('a'.repeat(40 * 1024))
    terminal.parse()
    expect(socket.sent).toHaveLength(1)
    socket.bytes('b'.repeat(30 * 1024))
    terminal.parse()
    expect(socket.sent).toEqual([
      { type: 'ack', offset: 100 },
      { type: 'ack', offset: 100 + 70 * 1024 },
    ])
  })

  // A flood goes on while xterm parses a large snapshot. Uncounted, this output made the client
  // ack too little forever, so the daemon never caught it up again once it was behind.
  it('counts live output that comes after replayed, before the catch-up is parsed', async () => {
    const { socket, terminal } = await shown()
    snapshotOf(socket, { cols: 80, rows: 24 }, ['screen'], 100)
    socket.bytes('a'.repeat(ACK_EVERY_BYTES))
    expect(socket.sent).toEqual([])

    terminal.parse()

    expect(socket.sent).toEqual([
      { type: 'ack', offset: 100 },
      { type: 'ack', offset: 100 + ACK_EVERY_BYTES },
    ])
  })
})

describe('size claims (AC 42, C44)', () => {
  it('sends no resize after the catch-up of a client that did not claim', async () => {
    setPageHidden(true)
    const { socket, connect, terminal } = await shown()
    expect(connect).toHaveBeenCalledWith({ cols: 80, rows: 24, visible: false })
    // The shell has another size; the snapshot comes at that size.
    snapshotOf(socket, { cols: 100, rows: 30 }, ['screen'], 6)
    terminal.parse()
    expect([terminal.cols, terminal.rows]).toEqual([80, 24])
    expect(socket.sent).toEqual([{ type: 'ack', offset: 6 }])

    // Its first input claims, with its own size first.
    terminal.fire('data', 'l')
    terminal.fire('data', 's')
    expect(socket.sent).toEqual([
      { type: 'ack', offset: 6 },
      { type: 'resize', cols: 80, rows: 24 },
      { type: 'input', data: 'l' },
      { type: 'input', data: 's' },
    ])
  })
})

describe('visible messages (AC 46, C48)', () => {
  it('claims with its size when it comes on screen, never when the page comes back', async () => {
    const { session, socket, terminal } = await shown()
    session.setOnScreen(false)
    expect(socket.sent).toEqual([{ type: 'visible', visible: false }])
    terminal.fitSize = { cols: 90, rows: 24 }
    session.setOnScreen(true)
    expect(socket.sent.slice(1)).toEqual([
      { type: 'resize', cols: 90, rows: 24 },
      { type: 'visible', visible: true, cols: 90, rows: 24 },
    ])

    socket.sent.length = 0
    setPageHidden(true)
    expect(socket.sent).toEqual([{ type: 'visible', visible: false }])
    setPageHidden(false)
    expect(socket.sent).toEqual([
      { type: 'visible', visible: false },
      { type: 'visible', visible: true },
    ])
  })

  it('claims only after the catch-up when it comes on screen during one', async () => {
    const { session, socket, terminal } = await shown()
    session.setOnScreen(false)
    snapshotOf(socket, { cols: 100, rows: 30 }, ['screen'])
    terminal.parse()
    socket.sent.length = 0

    session.setOnScreen(true)
    expect(socket.sent).toEqual([{ type: 'visible', visible: true }])
    socket.message({ type: 'replayed', offset: 6 })
    terminal.parse()
    expect(socket.sent).toEqual([
      { type: 'visible', visible: true },
      { type: 'ack', offset: 6 },
      { type: 'visible', visible: true, cols: 80, rows: 24 },
    ])
  })
})

describe('the end of a catch-up by exit or dispose (C4)', () => {
  it('shows the screen when the shell exits during a catch-up', async () => {
    const onEnd = vi.fn()
    const { session, socket, terminal } = await shown(onEnd)
    snapshotOf(socket, { cols: 80, rows: 24 }, ['screen'])
    socket.message({ type: 'exit', code: 0 })

    expect(session.state()).toMatchObject({ status: 'ended', catchingUp: false })
    expect(terminal.element?.parentElement?.style.visibility).toBe('')
    expect(onEnd).toHaveBeenCalledTimes(1)
  })

  it('leaves no timer and does nothing after a dispose during a catch-up', async () => {
    const { session, socket, terminal } = await shown()
    snapshotOf(socket, { cols: 100, rows: 30 }, ['screen'])
    terminal.element?.dispatchEvent(new KeyboardEvent('keydown', { bubbles: true }))
    socket.message({ type: 'replayed', offset: 6 })
    session.dispose()
    // Only the one that frees xterm.
    expect(vi.getTimerCount()).toBe(1)

    terminal.parse()
    vi.advanceTimersByTime(CATCH_UP_TIMEOUT_MS)
    expect(terminal.resets).toBe(0)
    expect(socket.sent).toEqual([{ type: 'visible', visible: false }])
  })
})

// A session whose every connect gets a new socket. Jitter 0.5 means none: the delays are exact.
function reconnecting(random = () => 0.5) {
  const sockets: FakeSocket[] = []
  const connect = vi.fn(async (_query: TerminalConnectQuery) => {
    const socket = fakeSocket()
    sockets.push(socket)
    return socket.asWebSocket()
  })
  const onEnd = vi.fn()
  const session = createTerminalSession({ connect, onEnd, random })
  const terminal = session.terminal as unknown as FakeTerminal
  const last = () => sockets.at(-1) as FakeSocket
  return { session, sockets, connect, onEnd, terminal, last }
}

// That session on screen, its first socket open and caught up at offset 6.
async function attached(random?: () => number) {
  const made = reconnecting(random)
  made.session.show(box())
  await flush()
  const socket = made.last()
  socket.open()
  snapshotOf(socket, { cols: 80, rows: 24 }, ['screen'], 6)
  made.terminal.parse()
  return { ...made, socket }
}

// Lets the planned retry run, and its connect() reach the socket.
async function wait(ms: number) {
  vi.advanceTimersByTime(ms)
  await flush()
}

describe('reconnecting a lost socket (AC 27, C29)', () => {
  it('resumes the same shell from its offset, in place, and sends only an ack', async () => {
    const { session, socket, connect, terminal, last } = await attached()
    socket.bytes('live')
    terminal.parse()

    socket.drop()
    expect(session.state().status).toBe('reconnecting')
    await wait(499)
    expect(connect).toHaveBeenCalledTimes(1)
    await wait(1)
    expect(connect).toHaveBeenCalledTimes(2)
    expect(connect).toHaveBeenLastCalledWith({
      cols: 80,
      rows: 24,
      visible: true,
      shellId: 'shell-1',
      offset: 10,
    })

    const next = last()
    next.open()
    expect(session.state().status).toBe('open')
    next.message({ type: 'hello', shellId: 'shell-1' })
    next.message({ type: 'resume', offset: 10 })
    next.bytes('missed')
    next.message({ type: 'replayed', offset: 16 })
    terminal.parse()

    // No reset: the screen stays and gets the missed bytes after the old ones.
    expect(terminal.resets).toBe(1)
    expect(terminal.written.filter(Boolean)).toEqual(['screen', 'live', 'missed'])
    expect(session.state().catchingUp).toBe(false)
    // A resume never claims the size, so nothing but the ack goes out.
    expect(next.sent).toEqual([{ type: 'ack', offset: 16 }])
  })

  it('reconnects a hidden cached terminal too, as not visible', async () => {
    const { session, socket, connect } = await attached()
    session.hide(box())
    socket.drop()
    await wait(500)

    expect(connect).toHaveBeenLastCalledWith({
      cols: 80,
      rows: 24,
      visible: false,
      shellId: 'shell-1',
      offset: 6,
    })
  })

  it('tries again when connect fails, with the next delay', async () => {
    const { socket, connect } = await attached()
    socket.drop()
    connect.mockRejectedValueOnce(new Error('The daemon is down.'))
    await wait(500)
    expect(connect).toHaveBeenCalledTimes(2)

    await wait(999)
    expect(connect).toHaveBeenCalledTimes(2)
    await wait(1)
    expect(connect).toHaveBeenCalledTimes(3)
  })
})

describe('what a reconnect asks for (AC 43, C45)', () => {
  it('attaches plainly after a socket lost before hello', async () => {
    const { session, connect, last } = reconnecting()
    session.show(box())
    await flush()
    last().open()
    last().drop()
    await wait(500)

    expect(connect).toHaveBeenLastCalledWith({ cols: 80, rows: 24, visible: true })
  })

  it('resumes with the shell id and no offset after a socket lost during its catch-up', async () => {
    const { session, connect, last, terminal } = reconnecting()
    session.show(box())
    await flush()
    const socket = last()
    socket.open()
    snapshotOf(socket, { cols: 80, rows: 24 }, ['screen'])
    terminal.parse()
    socket.drop()
    await wait(500)

    expect(connect).toHaveBeenLastCalledWith({
      cols: 80,
      rows: 24,
      visible: true,
      shellId: 'shell-1',
    })
  })

  it('keeps keys typed during a catch-up across a reconnect, for the next catch-up', async () => {
    const { session, last, terminal } = reconnecting()
    session.show(box())
    await flush()
    const socket = last()
    socket.open()
    snapshotOf(socket, { cols: 80, rows: 24 }, ['screen'])
    terminal.parse()
    terminal.fire('data', 'a')
    terminal.fire('data', 'á')
    socket.drop()

    // The old catch-up's timer must not end it while the socket is down.
    await wait(CATCH_UP_TIMEOUT_MS)
    expect(session.state().catchingUp).toBe(true)
    const next = last()
    next.open()
    snapshotOf(next, { cols: 80, rows: 24 }, ['screen again'], 12)
    terminal.parse()

    expect(socket.sent).toEqual([])
    // A resume did not claim, so the first input claims with its size first.
    expect(next.sent).toEqual([
      { type: 'ack', offset: 12 },
      { type: 'resize', cols: 80, rows: 24 },
      { type: 'input', data: 'a' },
      { type: 'input', data: 'á' },
    ])
  })
})

describe('when a lost socket does not come back (AC 29, AC 30, C31, C32)', () => {
  it('makes no new socket after the shell exits', async () => {
    const { session, socket, connect } = await attached()
    socket.message({ type: 'exit', code: 0 })
    socket.drop(1000)
    await wait(60_000)

    expect(session.state().status).toBe('ended')
    expect(connect).toHaveBeenCalledTimes(1)
  })

  it.each([CLOSE_UNAUTHORIZED, CLOSE_REFUSED, CLOSE_GONE])(
    'makes no new socket after close code %i',
    async (code) => {
      const { session, socket, connect, onEnd } = await attached()
      socket.drop(code)
      await wait(60_000)

      expect(session.state().status).toBe('ended')
      expect(connect).toHaveBeenCalledTimes(1)
      expect(onEnd).toHaveBeenCalledTimes(1)
    },
  )

  it('ends when the resume finds the shell gone, and shows the screen', async () => {
    const { session, connect, onEnd, last, terminal } = reconnecting()
    session.show(box())
    await flush()
    last().open()
    snapshotOf(last(), { cols: 80, rows: 24 }, ['screen'])
    terminal.parse()
    last().drop()
    await wait(500)

    last().open()
    last().drop(CLOSE_GONE)
    await wait(60_000)

    expect(session.state()).toMatchObject({ status: 'ended', catchingUp: false })
    expect(connect).toHaveBeenCalledTimes(2)
    expect(onEnd).toHaveBeenCalledTimes(1)
  })
})

describe('the retry schedule (AC 31, C33)', () => {
  // Every try fails before its socket opens, as when the daemon is down.
  async function failing(random: () => number, delays: number[]) {
    const { socket, connect, last } = await attached(random)
    socket.drop()
    for (const [index, delay] of delays.entries()) {
      await wait(delay - 1)
      expect(connect).toHaveBeenCalledTimes(index + 1)
      await wait(1)
      expect(connect).toHaveBeenCalledTimes(index + 2)
      last().drop()
    }
  }

  it('doubles the delay from 0.5 s up to 10 s', async () => {
    await failing(() => 0.5, [500, 1000, 2000, 4000, 8000, 10_000, 10_000])
  })

  it('takes 20% off with the lowest jitter', async () => {
    await failing(() => 0, [400, 800, 1600, 3200, 6400, 8000, 8000])
  })

  it('adds 20% with the highest jitter', async () => {
    await failing(() => 0.999_999_9, [600, 1200, 2400, 4800, 9600, 12_000])
  })

  it('starts from the first delay again after a try that worked', async () => {
    const { socket, connect, last } = await attached()
    socket.drop()
    await wait(500)
    last().drop()
    await wait(1000)
    const back = last()
    back.open()
    back.message({ type: 'hello', shellId: 'shell-1' })
    back.drop()

    await wait(499)
    expect(connect).toHaveBeenCalledTimes(3)
    await wait(1)
    expect(connect).toHaveBeenCalledTimes(4)
  })

  it('tries at once on online, once, even with a try planned', async () => {
    const { socket, connect } = await attached()
    socket.drop()
    window.dispatchEvent(new Event('online'))
    window.dispatchEvent(new Event('online'))
    await flush()
    expect(connect).toHaveBeenCalledTimes(2)

    // The planned try is gone, and the one on its way is not doubled.
    await wait(60_000)
    expect(connect).toHaveBeenCalledTimes(2)
  })

  it('does not try while the page is hidden, and tries at once when it comes back', async () => {
    const { socket, connect } = await attached()
    socket.drop()
    // Hidden with a try planned: it is dropped.
    setPageHidden(true)
    await wait(60_000)
    window.dispatchEvent(new Event('online'))
    await flush()
    expect(connect).toHaveBeenCalledTimes(1)

    setPageHidden(false)
    await flush()
    expect(connect).toHaveBeenCalledTimes(2)
    expect(connect).toHaveBeenLastCalledWith({
      cols: 80,
      rows: 24,
      visible: true,
      shellId: 'shell-1',
      offset: 6,
    })
  })

  it('plans no try for a socket lost while the page is hidden', async () => {
    const { socket, connect } = await attached()
    setPageHidden(true)
    socket.drop()
    await wait(60_000)

    expect(connect).toHaveBeenCalledTimes(1)
  })
})

describe('the heartbeat (AC 32, C34)', () => {
  it('pings every HEARTBEAT_MS, and reconnects when no pong came for two of them', async () => {
    const { session, socket, connect } = await attached()
    socket.answerPings = false

    await wait(HEARTBEAT_MS - 1)
    expect(socket.pings).toBe(0)
    await wait(1)
    expect(socket.pings).toBe(1)
    await wait(2 * HEARTBEAT_MS - 1)
    expect(socket.pings).toBe(2)
    expect(socket.closed).toBe(false)
    await wait(1)
    expect(socket.closed).toBe(true)
    expect(session.state().status).toBe('reconnecting')

    await wait(500)
    expect(connect).toHaveBeenCalledTimes(2)
  })

  it('keeps a socket whose pongs come', async () => {
    const { session, socket } = await attached()
    await wait(10 * HEARTBEAT_MS)

    expect(socket.pings).toBe(10)
    expect(socket.closed).toBe(false)
    expect(session.state().status).toBe('open')
  })

  it('does not ping while the page is hidden', async () => {
    const { socket } = await attached()
    socket.answerPings = false
    setPageHidden(true)
    await wait(10 * HEARTBEAT_MS)
    expect(socket.pings).toBe(0)
    expect(socket.closed).toBe(false)

    setPageHidden(false)
    await wait(HEARTBEAT_MS)
    expect(socket.pings).toBe(1)
  })

  it('stops pinging when disposed', async () => {
    const { session, socket } = await attached()
    session.dispose()
    await wait(10 * HEARTBEAT_MS)

    expect(socket.pings).toBe(0)
  })
})
