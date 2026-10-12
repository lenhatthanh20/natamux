import type { TerminalSize } from '@natamux/core'
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { gpuBudget } from '../lib/gpu-renderer'
import { cachedTerminalCount, HOLDER_ID, resetTerminalCache } from '../lib/terminal-cache'
import { fontStack, saveTerminalFont } from '../lib/terminal-font'
import { DISPOSE_DELAY_MS } from '../lib/terminal-session'
import { terminalTheme } from '../lib/terminal-theme'
import {
  type FakeSocket,
  type FakeTerminal,
  fakeSocket,
  fakes,
  resetFakes,
} from '../lib/xterm-fake.fixture'
import TerminalView from './TerminalView'

vi.mock('@xterm/xterm', async () => ({
  Terminal: (await import('../lib/xterm-fake.fixture')).FakeTerminal,
}))
vi.mock('@xterm/addon-fit', async () => ({
  FitAddon: (await import('../lib/xterm-fake.fixture')).FakeFitAddon,
}))
vi.mock('@xterm/addon-search', async () => ({
  SearchAddon: (await import('../lib/xterm-fake.fixture')).FakeSearchAddon,
}))
vi.mock('@xterm/addon-unicode11', async () => ({
  Unicode11Addon: (await import('../lib/xterm-fake.fixture')).FakeAddon,
}))
vi.mock('@xterm/addon-web-links', async () => ({
  WebLinksAddon: (await import('../lib/xterm-fake.fixture')).FakeAddon,
}))
vi.mock('@xterm/addon-webgl', async () => ({
  WebglAddon: (await import('../lib/xterm-fake.fixture')).FakeWebglAddon,
}))

type Report = (entries: { isIntersecting: boolean }[]) => void
let reportVisible: Report = () => undefined

const KEY = 'T-0001/1'

const flush = () =>
  act(async () => {
    for (let step = 0; step < 5; step += 1) {
      await Promise.resolve()
    }
  })

const setVisible = (visible: boolean) => act(() => reportVisible([{ isIntersecting: visible }]))

// Each call of connect gets the next socket.
function sockets(count: number) {
  const made: FakeSocket[] = Array.from({ length: count }, fakeSocket)
  let next = 0
  const connect = vi.fn(async (_size: TerminalSize) => {
    const socket = made[next]
    next += 1
    if (socket === undefined) {
      throw new Error('No socket left.')
    }
    return socket.asWebSocket()
  })
  return { made, connect }
}

function lastTerminal(): FakeTerminal {
  const terminal = fakes.terminals.at(-1)
  if (terminal === undefined) {
    throw new Error('No terminal was made.')
  }
  return terminal
}

async function openTerminal(cacheKey?: string) {
  const { made, connect } = sockets(1)
  const socket = made[0] as FakeSocket
  const view = render(<TerminalView connect={connect} cacheKey={cacheKey} />)
  await flush()
  act(() => socket.open())
  return { socket, connect, terminal: lastTerminal(), view }
}

const holder = () => document.getElementById(HOLDER_ID)
const status = () => screen.queryByRole('status')?.textContent ?? null

beforeEach(() => {
  resetFakes()
  gpuBudget.reset()
  vi.useFakeTimers()
  vi.stubGlobal(
    'ResizeObserver',
    class {
      observe = () => undefined
      disconnect = () => undefined
    },
  )
  vi.stubGlobal(
    'IntersectionObserver',
    class {
      constructor(callback: Report) {
        reportVisible = callback
      }
      observe = () => undefined
      disconnect = () => undefined
    },
  )
})

afterEach(() => {
  cleanup()
  vi.restoreAllMocks()
  resetTerminalCache()
  vi.unstubAllGlobals()
  vi.useRealTimers()
  saveTerminalFont(null)
  document.documentElement.removeAttribute('data-theme')
})

describe('TerminalView resources', () => {
  it('closes its socket and disposes the terminal and its GPU renderer when it closes', async () => {
    const { socket, terminal, view } = await openTerminal()

    view.unmount()
    expect(socket.closed).toBe(true)
    act(() => vi.advanceTimersByTime(DISPOSE_DELAY_MS))

    expect(terminal.disposed).toBe(true)
    expect(fakes.webgl.length).toBeGreaterThan(0)
    expect(fakes.webgl.every((webgl) => webgl.disposed)).toBe(true)
  })

  it('keeps the GPU renderer while hidden, with no timer (AC 33)', async () => {
    await openTerminal()
    expect(fakes.webgl).toHaveLength(1)

    setVisible(false)
    act(() => vi.advanceTimersByTime(60_000))
    expect(fakes.webgl[0]?.disposed).toBe(false)

    setVisible(true)
    expect(fakes.webgl).toHaveLength(1)
  })

  it('keeps the output that arrives while hidden, in order', async () => {
    const { socket, terminal, connect, view } = await openTerminal(KEY)
    view.unmount()

    for (const data of ['one ', 'two ', 'three']) {
      act(() => socket.message({ type: 'output', data }))
    }
    render(<TerminalView connect={connect} cacheKey={KEY} />)

    expect(terminal.written).toEqual(['one ', 'two ', 'three'])
  })
})

describe('TerminalView in the cache', () => {
  it('keeps terminal and socket off screen, and reuses them (AC 1, 2)', async () => {
    const { socket, connect, terminal, view } = await openTerminal(KEY)
    expect(status()).toBeNull()

    view.unmount()
    expect(socket.closed).toBe(false)
    expect(holder()?.contains(terminal.element ?? null)).toBe(true)
    act(() => vi.advanceTimersByTime(DISPOSE_DELAY_MS * 5))
    expect(terminal.disposed).toBe(false)
    expect(cachedTerminalCount()).toEqual({ mounted: 0, hidden: 1 })

    const again = render(<TerminalView connect={connect} cacheKey={KEY} />)
    expect(status()).toBeNull()
    expect(fakes.terminals).toHaveLength(1)
    expect(connect).toHaveBeenCalledTimes(1)
    expect(again.container.contains(terminal.element ?? null)).toBe(true)
    expect(cachedTerminalCount()).toEqual({ mounted: 1, hidden: 0 })
  })

  it('sends no resize when a cached terminal is shown again (AC 1)', async () => {
    const { socket, connect, terminal, view } = await openTerminal(KEY)
    // As in the browser, the status strip takes room from the pane while it is on the page.
    Object.defineProperty(terminal, 'fitSize', {
      get: () => ({ cols: 80, rows: document.querySelector('[role="status"]') ? 22 : 24 }),
    })
    view.unmount()
    socket.sent.length = 0

    render(<TerminalView connect={connect} cacheKey={KEY} />)

    expect(terminal.rows).toBe(24)
    const resizes = socket.sent.filter((message) => (message as { type: string }).type === 'resize')
    expect(resizes).toEqual([])
  })

  it('moves the terminal from the agents grid to the task view in one commit (C2)', async () => {
    const { made, connect } = sockets(2)
    const view = render(
      <section aria-label="Agents">
        <TerminalView connect={connect} cacheKey={KEY} />
      </section>,
    )
    await flush()
    act(() => made[0]?.open())
    const terminal = lastTerminal()

    // Another parent, as when the grid closes and the task view opens with the same shell.
    view.rerender(
      <main>
        <section aria-label="Shells">
          <TerminalView connect={connect} cacheKey={KEY} />
        </section>
      </main>,
    )
    await flush()

    expect(connect).toHaveBeenCalledTimes(1)
    expect(fakes.terminals).toHaveLength(1)
    expect(made[0]?.closed).toBe(false)
    const shells = screen.getByRole('region', { name: 'Shells' })
    expect(shells.contains(terminal.element ?? null)).toBe(true)
    expect(cachedTerminalCount()).toEqual({ mounted: 1, hidden: 0 })
  })

  it('gives two views of the same key their own terminal and socket', async () => {
    const { made, connect } = sockets(2)
    render(
      <>
        <TerminalView connect={connect} cacheKey={KEY} />
        <TerminalView connect={connect} cacheKey={KEY} />
      </>,
    )
    await flush()
    act(() => {
      made[0]?.open()
      made[1]?.open()
    })

    expect(fakes.terminals).toHaveLength(2)
    expect(connect).toHaveBeenCalledTimes(2)
    const [first, second] = fakes.terminals
    expect(first?.element).not.toBe(second?.element)
    cleanup()
    expect(made[1]?.closed).toBe(true)
    expect(made[0]?.closed).toBe(false)
  })

  it('takes the theme and font changed while it was hidden (AC 6)', async () => {
    document.documentElement.dataset.theme = 'light'
    const { connect, terminal, view } = await openTerminal(KEY)
    expect(terminal.options.theme).toEqual(terminalTheme('light'))
    view.unmount()

    act(() => {
      document.documentElement.dataset.theme = 'dark'
      saveTerminalFont({ family: 'Fira Code', size: 16 })
    })
    render(<TerminalView connect={connect} cacheKey={KEY} />)

    expect(fakes.terminals).toHaveLength(1)
    expect(terminal.options.theme).toEqual(terminalTheme('dark'))
    expect(terminal.options.fontFamily).toBe(fontStack('Fira Code'))
    expect(terminal.options.fontSize).toBe(16)
  })

  it('starts a fresh terminal on "Start a new shell" and disposes the old one (AC 5)', async () => {
    const { made, connect } = sockets(2)
    render(<TerminalView connect={connect} cacheKey={KEY} />)
    await flush()
    act(() => made[0]?.open())
    const old = lastTerminal()

    act(() => made[0]?.message({ type: 'exit', code: 0 }))
    expect(status()).toContain('The shell ended.')
    fireEvent.click(screen.getByRole('button', { name: 'Start a new shell' }))
    await flush()

    expect(made[0]?.closed).toBe(true)
    expect(connect).toHaveBeenCalledTimes(2)
    expect(fakes.terminals).toHaveLength(2)
    act(() => made[1]?.open())
    expect(status()).toBeNull()
    act(() => vi.advanceTimersByTime(DISPOSE_DELAY_MS))
    expect(old.disposed).toBe(true)
    expect(lastTerminal().disposed).toBe(false)
    expect(cachedTerminalCount()).toEqual({ mounted: 1, hidden: 0 })
  })

  it('starts one new shell when "Start a new shell" is clicked twice at once (C6)', async () => {
    const { made, connect } = sockets(3)
    render(<TerminalView connect={connect} cacheKey={KEY} />)
    await flush()
    act(() => made[0]?.open())
    const old = lastTerminal()
    const disposeOld = vi.spyOn(old, 'dispose')
    act(() => made[0]?.message({ type: 'exit', code: 0 }))
    const errors = vi.spyOn(console, 'error')

    const button = screen.getByRole('button', { name: 'Start a new shell' })
    act(() => {
      fireEvent.click(button)
      fireEvent.click(button)
    })
    await flush()

    expect(connect).toHaveBeenCalledTimes(2)
    expect(fakes.terminals).toHaveLength(2)
    expect(made[0]?.closed).toBe(true)
    act(() => vi.advanceTimersByTime(DISPOSE_DELAY_MS))
    expect(disposeOld).toHaveBeenCalledTimes(1)
    expect(errors).not.toHaveBeenCalled()
    expect(cachedTerminalCount()).toEqual({ mounted: 1, hidden: 0 })
  })

  it('disposes a hidden terminal at once when its shell ends', async () => {
    const { socket, terminal, view } = await openTerminal(KEY)
    view.unmount()

    act(() => socket.message({ type: 'exit', code: 0 }))
    expect(socket.closed).toBe(true)
    expect(cachedTerminalCount()).toEqual({ mounted: 0, hidden: 0 })
    act(() => vi.advanceTimersByTime(DISPOSE_DELAY_MS))
    expect(terminal.disposed).toBe(true)
  })

  it('disposes a hidden terminal at once when its shell reports an error (C6)', async () => {
    const { socket, terminal, view } = await openTerminal(KEY)
    view.unmount()

    act(() => socket.message({ type: 'error', message: 'No shell.' }))
    expect(socket.closed).toBe(true)
    expect(cachedTerminalCount()).toEqual({ mounted: 0, hidden: 0 })
    act(() => vi.advanceTimersByTime(DISPOSE_DELAY_MS))
    expect(terminal.disposed).toBe(true)
  })

  it('disposes a terminal whose shell ended when it leaves the screen', async () => {
    const { socket, terminal, view } = await openTerminal(KEY)
    act(() => socket.drop())
    expect(status()).toContain('The shell ended.')

    view.unmount()
    expect(cachedTerminalCount()).toEqual({ mounted: 0, hidden: 0 })
    act(() => vi.advanceTimersByTime(DISPOSE_DELAY_MS))
    expect(terminal.disposed).toBe(true)
  })

  it('shows "Jump to latest" when scrolled up and goes to the bottom on click', async () => {
    const frames: FrameRequestCallback[] = []
    vi.stubGlobal('requestAnimationFrame', (callback: FrameRequestCallback) => {
      frames.push(callback)
      return frames.length
    })
    vi.stubGlobal('cancelAnimationFrame', () => undefined)
    const { terminal } = await openTerminal(KEY)
    terminal.buffer.active = { viewportY: 0, baseY: 10 }
    act(() => terminal.fire('writeParsed'))
    act(() => {
      for (const callback of frames.splice(0)) {
        callback(0)
      }
    })

    fireEvent.click(screen.getByRole('button', { name: 'Jump to latest' }))
    expect(terminal.buffer.active.viewportY).toBe(10)
  })
})
