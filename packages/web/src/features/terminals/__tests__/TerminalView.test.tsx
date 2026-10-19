import { CLOSE_GONE, type TaskState } from '@natamux/core'
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest'
import { fireAppEvent } from '../../../lib/app-events'
import { GIVE_BACK_DELAY_MS, gpuBudget } from '../gpu-renderer'
import { DISPOSE_DELAY_MS } from '../session/constants'
import { importWebgl } from '../session/webgl'
import TerminalView, { RESTORE_FADE_MS } from '../TerminalView'
import type { TerminalConnectQuery } from '../terminal'
import { cachedTerminalCount, HOLDER_ID, resetTerminalCache } from '../terminal-cache'
import { fontStack, saveTerminalFont } from '../terminal-font'
import { terminalTheme } from '../terminal-theme'
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
  const connect = vi.fn(async (_query: TerminalConnectQuery) => {
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

// Loaded once, so every GPU renderer starts in the same call, as on a page that had one before.
beforeAll(async () => {
  await importWebgl()
})

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

  it('keeps the GPU renderer when shown again before the give-back delay', async () => {
    await openTerminal()
    expect(fakes.webgl).toHaveLength(1)

    setVisible(false)
    act(() => vi.advanceTimersByTime(GIVE_BACK_DELAY_MS - 1))
    setVisible(true)
    act(() => vi.advanceTimersByTime(GIVE_BACK_DELAY_MS))
    expect(fakes.webgl).toHaveLength(1)
    expect(fakes.webgl[0]?.disposed).toBe(false)
  })

  it('gives the GPU renderer back while hidden, and takes one again when shown', async () => {
    await openTerminal()
    setVisible(false)
    act(() => vi.advanceTimersByTime(GIVE_BACK_DELAY_MS))
    expect(fakes.webgl[0]?.disposed).toBe(true)

    setVisible(true)
    expect(fakes.webgl).toHaveLength(2)
    expect(fakes.webgl[1]?.disposed).toBe(false)
  })

  it('keeps the output that arrives while hidden, in order', async () => {
    const { socket, terminal, connect, view } = await openTerminal(KEY)
    view.unmount()

    for (const data of ['one ', 'two ', 'three']) {
      act(() => socket.bytes(data))
    }
    render(<TerminalView connect={connect} cacheKey={KEY} />)

    expect(terminal.written).toEqual(['one ', 'two ', 'three'])
  })

  it('shows the skeleton while a catch-up is written, and the screen after it (AC 9)', async () => {
    const { socket, terminal } = await openTerminal()
    const loading = () => screen.queryByRole('status', { name: 'Loading the terminal' })
    expect(loading()).toBeNull()

    act(() => socket.message({ type: 'snapshot', cols: 80, rows: 24 }))
    expect(loading()).not.toBeNull()
    act(() => {
      socket.bytes('screen')
      socket.message({ type: 'replayed', offset: 6 })
    })
    expect(loading()).not.toBeNull()

    act(() => terminal.parse())
    expect(loading()).toBeNull()
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

  it('starts a fresh terminal on "Open a new shell" and disposes the old one (AC 5)', async () => {
    const { made, connect } = sockets(2)
    render(<TerminalView connect={connect} cacheKey={KEY} />)
    await flush()
    act(() => made[0]?.open())
    const old = lastTerminal()

    act(() => made[0]?.message({ type: 'exit', code: 0 }))
    expect(status()).toContain('The shell ended.')
    fireEvent.click(screen.getByRole('button', { name: 'Open a new shell' }))
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

  it('starts one new shell when "Open a new shell" is clicked twice at once (C6)', async () => {
    const { made, connect } = sockets(3)
    render(<TerminalView connect={connect} cacheKey={KEY} />)
    await flush()
    act(() => made[0]?.open())
    const old = lastTerminal()
    const disposeOld = vi.spyOn(old, 'dispose')
    act(() => made[0]?.message({ type: 'exit', code: 0 }))
    const errors = vi.spyOn(console, 'error')

    const button = screen.getByRole('button', { name: 'Open a new shell' })
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
    act(() => socket.drop(CLOSE_GONE))
    expect(status()).toContain('The shell ended.')

    view.unmount()
    expect(cachedTerminalCount()).toEqual({ mounted: 0, hidden: 0 })
    act(() => vi.advanceTimersByTime(DISPOSE_DELAY_MS))
    expect(terminal.disposed).toBe(true)
  })

  // AC 27: the strip shows while the socket comes back, and goes once it is open.
  it('shows "Reconnecting." while a lost socket comes back, with no new shell offered', async () => {
    const { made, connect } = sockets(2)
    render(<TerminalView connect={connect} cacheKey={KEY} />)
    await flush()
    act(() => made[0]?.open())
    act(() => made[0]?.message({ type: 'hello', shellId: 'shell-1' }))

    act(() => made[0]?.drop())
    expect(status()).toBe('Reconnecting.')
    expect(screen.queryByRole('button', { name: 'Open a new shell' })).toBeNull()
    act(() => vi.advanceTimersByTime(600))
    await flush()
    expect(connect).toHaveBeenCalledTimes(2)
    act(() => made[1]?.open())
    expect(status()).toBeNull()
    expect(fakes.terminals).toHaveLength(1)
  })

  // AC 29: a resume that finds the shell gone.
  it('shows "The shell ended." and "Open a new shell" when the shell is gone', async () => {
    const { made, connect } = sockets(2)
    const view = render(<TerminalView connect={connect} cacheKey={KEY} />)
    await flush()
    act(() => made[0]?.open())
    act(() => made[0]?.message({ type: 'hello', shellId: 'shell-1' }))
    act(() => made[0]?.drop())
    act(() => vi.advanceTimersByTime(600))
    await flush()

    act(() => made[1]?.open())
    act(() => made[1]?.drop(CLOSE_GONE))
    expect(status()).toContain('The shell ended.')
    expect(screen.getByRole('button', { name: 'Open a new shell' })).toBeTruthy()
    act(() => vi.advanceTimersByTime(60_000))
    expect(connect).toHaveBeenCalledTimes(2)
    // The cache drops it once it leaves the screen.
    view.unmount()
    expect(cachedTerminalCount()).toEqual({ mounted: 0, hidden: 0 })
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

const exit = (socket: FakeSocket | undefined, code: number | null = 0) =>
  act(() => socket?.message({ type: 'exit', code }))
const newShell = () => screen.queryByRole('button', { name: 'Open a new shell' })
const dimmed = () =>
  screen.getByRole('region', { name: 'Terminal screen' }).classList.contains('opacity-60')
const inputs = (socket: FakeSocket | undefined) =>
  (socket?.sent ?? []).filter((message) => (message as { type: string }).type === 'input')

// An open terminal of a task in this state, with sockets for the shells opened after it.
async function taskTerminal(taskState?: TaskState, count = 2) {
  const { made, connect } = sockets(count)
  const view = render(<TerminalView connect={connect} cacheKey={KEY} taskState={taskState} />)
  await flush()
  act(() => made[0]?.open())
  return { made, connect, view, terminal: lastTerminal() }
}

async function openNewShell(made: FakeSocket[]) {
  fireEvent.click(newShell() as HTMLElement)
  await flush()
  act(() => made[1]?.open())
}

const ENDED = 'The shell ended.'
const FINISHED = ['done', 'cancelled'] as const

describe('TerminalView after the task ends', () => {
  // The finished banner says how the task ended, once for all its shells.
  it.each(FINISHED)('says only that the shell ended for a %s task', async (state) => {
    const { made } = await taskTerminal(state)
    exit(made[0], 137)

    expect(screen.getAllByRole('status')).toHaveLength(1)
    expect(status()).toBe(ENDED)
    expect(screen.queryByText(/shells were closed/)).toBeNull()
    expect(newShell()).not.toBeNull()
  })

  it.each(FINISHED)('says the shell ended when it is gone, for a %s task', async (state) => {
    const { made } = await taskTerminal(state)
    act(() => made[0]?.drop(CLOSE_GONE))

    expect(status()).toBe(ENDED)
    expect(newShell()).not.toBeNull()
  })

  it('drops the exit code once the task state says the task finished', async () => {
    const { made, connect, view } = await taskTerminal('running')
    exit(made[0], 1)
    expect(status()).toBe('The shell ended. Exit code 1.')

    view.rerender(<TerminalView connect={connect} cacheKey={KEY} taskState="done" />)
    expect(status()).toBe(ENDED)
  })
})

describe('TerminalView exit report', () => {
  it.each([0, 1, null])('tells its pane once that the shell ended with code %s', async (code) => {
    const onExit = vi.fn()
    const { made, connect } = sockets(2)
    render(<TerminalView connect={connect} cacheKey={KEY} onExit={onExit} />)
    await flush()
    act(() => made[0]?.open())
    expect(onExit).not.toHaveBeenCalled()

    exit(made[0], code)
    expect(onExit).toHaveBeenCalledTimes(1)
    expect(onExit).toHaveBeenCalledWith(code)
  })
})

describe('TerminalView after the shell ends by itself', () => {
  it.each([
    [0, 'The shell ended.'],
    [null, 'The shell ended.'],
    [1, 'The shell ended. Exit code 1.'],
    [2, 'The shell ended. Exit code 2.'],
    [130, 'The shell ended. Exit code 130.'],
    [-1, 'The shell ended. Exit code -1.'],
  ])('shows exit code %s as "%s"', async (code, text) => {
    const { made } = await taskTerminal('running')
    exit(made[0], code)
    expect(status()).toBe(text)
  })

  it('says the shell ended when it is gone with no exit frame and no task state', async () => {
    const { made } = await taskTerminal()
    act(() => made[0]?.drop(CLOSE_GONE))
    expect(status()).toBe('The shell ended.')
    expect(newShell()).not.toBeNull()
  })

  it('keeps the exit code through a re-render', async () => {
    const { made, connect, view } = await taskTerminal('running')
    exit(made[0], 3)
    view.rerender(<TerminalView connect={connect} cacheKey={KEY} taskState="review" />)
    expect(status()).toBe('The shell ended. Exit code 3.')
  })
})

describe('TerminalView read-only after the end', () => {
  it('keeps the screen, dims it, hides the cursor and takes no more input', async () => {
    const { made, terminal } = await taskTerminal('done')
    act(() => made[0]?.bytes('> old prompt'))
    expect(dimmed()).toBe(false)
    exit(made[0])

    expect(terminal.written).toContain('> old prompt')
    expect(terminal.options.disableStdin).toBe(true)
    expect(terminal.options.cursorBlink).toBe(false)
    expect(terminal.written.at(-1)).toContain('\x1b[?25l')
    expect(dimmed()).toBe(true)
  })

  it('sends no input after the exit frame, even before xterm stops taking keys', async () => {
    const { made, terminal } = await taskTerminal('done')
    act(() => terminal.fire('data', 'a'))
    expect(inputs(made[0])).toHaveLength(1)

    exit(made[0])
    for (const data of ['ls', '\r', 'pasted text']) {
      act(() => terminal.fire('data', data))
    }
    expect(inputs(made[0])).toHaveLength(1)
  })
})

describe('TerminalView open a new shell', () => {
  it('says what the button does, in a status bar', async () => {
    const { made } = await taskTerminal('done')
    exit(made[0])

    const button = newShell()
    expect(button?.tagName).toBe('BUTTON')
    expect(button?.getAttribute('data-tip')).toBe(
      "Starts a plain shell in the task's workspace. It does not start the agent.",
    )
    expect(screen.queryByRole('button', { name: 'Start a new shell' })).toBeNull()
    expect(screen.getByRole('status').tagName).toBe('P')
  })

  it('opens a new shell that takes input, with the cursor back and no bar', async () => {
    const { made, connect } = await taskTerminal('done')
    exit(made[0])

    await openNewShell(made)
    const terminal = lastTerminal()

    expect(connect).toHaveBeenCalledTimes(2)
    expect(terminal.options.disableStdin).not.toBe(true)
    expect(terminal.options.cursorBlink).toBe(true)
    expect(newShell()).toBeNull()
    expect(status()).toBeNull()
    expect(dimmed()).toBe(false)
    act(() => terminal.fire('data', 'pwd\r'))
    expect(inputs(made[1])).toEqual([{ type: 'input', data: 'pwd\r' }])
  })

  it('forgets the old exit code in the new shell', async () => {
    const { made } = await taskTerminal('running')
    exit(made[0], 1)
    await openNewShell(made)

    exit(made[1], 0)
    expect(status()).toBe('The shell ended.')
  })

  it('says the new shell ended when it ends while the task is done', async () => {
    const { made } = await taskTerminal('done')
    exit(made[0])
    await openNewShell(made)

    exit(made[1], 2)
    expect(status()).toBe(ENDED)
  })

  it('shows the error at the top when the new shell cannot start', async () => {
    // One socket only: the second connect fails.
    const { made } = await taskTerminal('done', 1)
    exit(made[0])

    fireEvent.click(newShell() as HTMLElement)
    await flush()

    expect(status()).toBe('The terminal could not start. No socket left.')
    expect(screen.getByRole('button', { name: 'Start a new shell' })).not.toBeNull()
    expect(newShell()).toBeNull()
    expect(dimmed()).toBe(false)
  })
})

describe('TerminalView before the shell opens', () => {
  it('shows "Connecting." at the top while the socket opens', async () => {
    const { connect } = sockets(1)
    render(<TerminalView connect={connect} taskState="done" />)
    await flush()

    expect(status()).toBe('Connecting.')
    expect(newShell()).toBeNull()
    expect(lastTerminal().options.disableStdin).not.toBe(true)
  })

  it('draws the status strip over the screen, so the screen keeps its box', async () => {
    const { made, connect } = sockets(1)
    render(<TerminalView connect={connect} />)
    await flush()

    const strip = screen.getByRole('status').parentElement
    const screenArea = screen.getByRole('region', { name: 'Terminal screen' })
    expect(strip?.className).toContain('absolute')
    expect(strip?.parentElement).toBe(screenArea.parentElement)

    act(() => made[0]?.open())
    expect(screen.queryByRole('status')).toBeNull()
  })

  it('shows the error at the top when the socket closes before it opens', async () => {
    const { made, connect } = sockets(1)
    render(<TerminalView connect={connect} taskState="done" />)
    await flush()
    act(() => made[0]?.drop())

    expect(status()).toBe('The terminal could not start. Could not connect to the terminal.')
    expect(screen.getByRole('button', { name: 'Start a new shell' })).not.toBeNull()
    expect(newShell()).toBeNull()
    expect(dimmed()).toBe(false)
  })
})

describe('TerminalView find', () => {
  it('makes the search addon at the first find, and reuses it the next time', async () => {
    const { terminal } = await openTerminal()
    // Mod+F finds in the terminal that has the focus.
    const openFind = () => {
      act(() => terminal.textarea?.focus())
      act(() => fireAppEvent('terminal-find'))
      return screen.getByRole('textbox', { name: 'Find' })
    }
    expect(fakes.searches).toHaveLength(0)

    fireEvent.keyDown(openFind(), { key: 'Escape' })
    expect(screen.queryByRole('textbox', { name: 'Find' })).toBeNull()
    openFind()

    expect(fakes.searches).toHaveLength(1)
  })
})

describe('TerminalView of a shell that comes back after a restart', () => {
  const LINE = 'Restoring GitHub Copilot…'
  const cover = () => screen.queryByText(LINE)?.closest<HTMLElement>('[role="status"]') ?? null

  it('covers the shell with a spinner until it is back, then fades it out', async () => {
    const { made, connect } = sockets(1)
    const view = render(<TerminalView connect={connect} restoring={LINE} />)
    await flush()
    act(() => made[0]?.open())

    const shown = cover()
    expect(shown?.querySelector('svg.hm-spin8')).toBeTruthy()
    expect(shown?.className).toContain('opacity-100')
    // An overlay: the terminal keeps its box, so it sends no resize.
    expect(shown?.className).toContain('absolute')

    view.rerender(<TerminalView connect={connect} />)
    expect(cover()?.className).toContain('opacity-0')
    act(() => vi.advanceTimersByTime(RESTORE_FADE_MS))
    expect(cover()).toBeNull()
  })

  it('drops the spinner when the shell cannot start', async () => {
    const { made, connect } = sockets(1)
    render(<TerminalView connect={connect} restoring={LINE} />)
    await flush()
    act(() => made[0]?.open())

    act(() => made[0]?.message({ type: 'error', message: 'At most 30 terminals can be open.' }))
    act(() => vi.advanceTimersByTime(RESTORE_FADE_MS))

    expect(cover()).toBeNull()
    expect(screen.getByRole('button', { name: 'Start a new shell' })).toBeTruthy()
  })

  it('shows the spinner of a connect late, so a quick connect does not flash it', async () => {
    const { connect } = sockets(1)
    render(<TerminalView connect={connect} />)
    await flush()

    const strip = screen.getByText('Connecting.').parentElement as HTMLElement
    expect(strip.querySelector('.hm-late-in svg.hm-spin8')).toBeTruthy()
  })
})

describe('a path dropped from the files explorer', () => {
  async function dropZone() {
    const { made, connect } = sockets(1)
    render(<TerminalView connect={connect} dropTaskId="T-0001" />)
    await flush()
    act(() => made[0]?.open())
    const zone = document.querySelector('.hm-terminal')
    if (zone === null) {
      throw new Error('No terminal on screen.')
    }
    return { zone, terminal: lastTerminal() }
  }

  const drag = (type: string, data: unknown) => ({
    dataTransfer: {
      types: [type, 'text/plain'],
      getData: (asked: string) => (asked === type ? JSON.stringify(data) : ''),
      dropEffect: 'none',
    },
  })

  it('pastes the path of its own task, quoted when needed, and takes the focus', async () => {
    const { zone, terminal } = await dropZone()
    const event = drag('application/x-natamux-path-t-0001', { path: 'docs/a b.md', kind: 'file' })
    const focuses = terminal.focuses

    fireEvent.dragOver(zone, event)
    expect(event.dataTransfer.dropEffect).toBe('copy')
    fireEvent.drop(zone, event)

    expect(terminal.pasted).toEqual(["'docs/a b.md' "])
    expect(terminal.focuses).toBe(focuses + 1)
  })

  it('refuses a path from the explorer of another task', async () => {
    const { zone, terminal } = await dropZone()
    const event = drag('application/x-natamux-path-t-0002', { path: 'src', kind: 'folder' })

    fireEvent.dragOver(zone, event)
    expect(event.dataTransfer.dropEffect).toBe('none')
    fireEvent.drop(zone, event)

    expect(terminal.pasted).toEqual([])
  })
})
