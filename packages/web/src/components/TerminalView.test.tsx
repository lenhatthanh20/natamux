import type { TaskState } from '@natamux/core'
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { RELEASE_DELAY_MS } from '../lib/gpu-renderer'
import TerminalView from './TerminalView'

type FakeTerminal = {
  written: string[]
  disposed: boolean
  options: Record<string, unknown>
  // What xterm calls with the keys the user types.
  typed?: (data: string) => void
}
type FakeWebgl = { disposed: boolean }

const fakes = vi.hoisted(() => ({
  terminals: [] as FakeTerminal[],
  webgl: [] as FakeWebgl[],
}))

vi.mock('@xterm/xterm', () => {
  const listener = () => ({ dispose: () => undefined })
  return {
    Terminal: class {
      options: Record<string, unknown> = {}
      cols = 80
      rows = 24
      unicode = { activeVersion: '' }
      buffer = { active: { viewportY: 0, baseY: 0 } }
      element: HTMLElement | undefined = undefined
      textarea = undefined
      written: string[] = []
      disposed = false
      typed: ((data: string) => void) | undefined = undefined
      constructor(options: Record<string, unknown> = {}) {
        this.options = { ...options }
        fakes.terminals.push(this)
      }
      loadAddon = () => undefined
      attachCustomKeyEventHandler = () => undefined
      open(parent: HTMLElement) {
        this.element = document.createElement('div')
        parent.append(this.element)
      }
      focus = () => undefined
      write(data: string) {
        this.written.push(data)
      }
      onData(callback: (data: string) => void) {
        this.typed = callback
        return { dispose: () => undefined }
      }
      onResize = listener
      onScroll = listener
      onWriteParsed = listener
      scrollToBottom = () => undefined
      dispose() {
        this.disposed = true
      }
    },
  }
})
vi.mock('@xterm/addon-fit', () => ({
  FitAddon: class {
    fit = () => undefined
  },
}))
vi.mock('@xterm/addon-search', () => ({ SearchAddon: class {} }))
vi.mock('@xterm/addon-unicode11', () => ({ Unicode11Addon: class {} }))
vi.mock('@xterm/addon-web-links', () => ({ WebLinksAddon: class {} }))
vi.mock('@xterm/addon-webgl', () => ({
  WebglAddon: class {
    disposed = false
    constructor() {
      fakes.webgl.push(this)
    }
    onContextLoss = () => undefined
    dispose() {
      this.disposed = true
    }
  },
}))

type Report = (entries: { isIntersecting: boolean }[]) => void
let reportVisible: Report = () => undefined

function fakeSocket() {
  return {
    readyState: 1,
    closed: false,
    onopen: null as null | (() => void),
    onmessage: null as null | ((event: { data: string }) => void),
    onclose: null as null | (() => void),
    onerror: null,
    sent: [] as string[],
    send(data: string) {
      this.sent.push(data)
    },
    close() {
      this.closed = true
    },
  }
}

const flush = () =>
  act(async () => {
    for (let step = 0; step < 5; step += 1) {
      await Promise.resolve()
    }
  })

const setVisible = (visible: boolean) => act(() => reportVisible([{ isIntersecting: visible }]))

type FakeSocket = ReturnType<typeof fakeSocket>

// Each connection gets its own socket, so a new shell can be told from the old one.
async function startTerminal(taskState?: TaskState) {
  const sockets: FakeSocket[] = []
  const connect = vi.fn(async () => {
    const socket = fakeSocket()
    sockets.push(socket)
    return socket as unknown as WebSocket
  })
  const view = render(<TerminalView connect={connect} taskState={taskState} />)
  await flush()
  const socket = sockets[0]
  const terminal = fakes.terminals.at(-1)
  if (socket === undefined || terminal === undefined) {
    throw new Error('No terminal was made.')
  }
  return { socket, sockets, terminal, view, connect }
}

async function openTerminal(taskState?: TaskState) {
  const started = await startTerminal(taskState)
  act(() => started.socket.onopen?.())
  return started
}

beforeEach(() => {
  fakes.terminals.length = 0
  fakes.webgl.length = 0
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
  vi.useRealTimers()
  vi.unstubAllGlobals()
})

describe('TerminalView resources', () => {
  it('closes its socket and disposes the terminal and its GPU renderer when it closes', async () => {
    const { socket, terminal, view } = await openTerminal()

    view.unmount()
    expect(socket.closed).toBe(true)
    act(() => vi.advanceTimersByTime(1000))

    expect(terminal.disposed).toBe(true)
    expect(fakes.webgl.every((webgl) => webgl.disposed)).toBe(true)
  })

  it('gives the GPU back while hidden, and takes it again when shown', async () => {
    await openTerminal()
    expect(fakes.webgl).toHaveLength(1)

    setVisible(false)
    act(() => vi.advanceTimersByTime(RELEASE_DELAY_MS))
    expect(fakes.webgl[0]?.disposed).toBe(true)

    setVisible(true)
    expect(fakes.webgl).toHaveLength(2)
    expect(fakes.webgl[1]?.disposed).toBe(false)
  })

  it('keeps the output that arrives while hidden, in order', async () => {
    const { socket, terminal } = await openTerminal()
    setVisible(false)
    act(() => vi.advanceTimersByTime(RELEASE_DELAY_MS))

    for (const data of ['one ', 'two ', 'three']) {
      act(() => socket.onmessage?.({ data: JSON.stringify({ type: 'output', data }) }))
    }
    setVisible(true)

    expect(terminal.written).toEqual(['one ', 'two ', 'three'])
  })
})

const exit = (socket: FakeSocket, code: number | null = 0) =>
  act(() => socket.onmessage?.({ data: JSON.stringify({ type: 'exit', code }) }))
const output = (socket: FakeSocket, data: string) =>
  act(() => socket.onmessage?.({ data: JSON.stringify({ type: 'output', data }) }))
const statusText = () => screen.getByRole('status').textContent
const newShell = () => screen.queryByRole('button', { name: 'Open a new shell' })
const dimmed = () =>
  screen.getByRole('region', { name: 'Terminal screen' }).classList.contains('opacity-60')
const inputs = (socket: FakeSocket) =>
  socket.sent.map((data) => JSON.parse(data)).filter((message) => message.type === 'input')

const DONE = 'Task done. The agent and its shells were closed.'
const CANCELLED = 'Task cancelled. The agent and its shells were closed.'

describe('TerminalView after the task ends', () => {
  it.each([
    ['done', DONE],
    ['cancelled', CANCELLED],
  ] as const)('says the task is %s, whatever the exit code', async (state, text) => {
    const { socket } = await openTerminal(state)
    exit(socket, 137)

    expect(screen.getAllByRole('status')).toHaveLength(1)
    expect(statusText()).toBe(text)
    expect(screen.queryByText('The shell ended.')).toBeNull()
    expect(newShell()).toBeTruthy()
  })

  it.each([
    ['done', DONE],
    ['cancelled', CANCELLED],
  ] as const)('says the task is %s when the socket closes with no exit', async (state, text) => {
    const { socket } = await openTerminal(state)
    act(() => socket.onclose?.())

    expect(statusText()).toBe(text)
    expect(newShell()).toBeTruthy()
  })

  it('follows a task state that arrives after the exit frame', async () => {
    const { socket, view, connect } = await openTerminal('running')
    exit(socket)
    expect(statusText()).toBe('The shell ended.')

    view.rerender(<TerminalView connect={connect} taskState="done" />)
    expect(statusText()).toBe(DONE)
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
    const { socket } = await openTerminal('running')
    exit(socket, code)
    expect(statusText()).toBe(text)
  })

  it('says the shell ended on a close with no exit frame and no task state', async () => {
    const { socket } = await openTerminal()
    act(() => socket.onclose?.())
    expect(statusText()).toBe('The shell ended.')
    expect(newShell()).toBeTruthy()
  })

  it('keeps the exit code through a re-render', async () => {
    const { socket, view, connect } = await openTerminal('running')
    exit(socket, 3)
    view.rerender(<TerminalView connect={connect} taskState="review" />)
    expect(statusText()).toBe('The shell ended. Exit code 3.')
  })
})

describe('TerminalView read-only after the end', () => {
  it('keeps the screen, dims it, hides the cursor and takes no more input', async () => {
    const { socket, terminal } = await openTerminal('done')
    output(socket, '> old prompt')
    expect(dimmed()).toBe(false)
    exit(socket)

    expect(terminal.written).toContain('> old prompt')
    expect(terminal.options.disableStdin).toBe(true)
    expect(terminal.options.cursorBlink).toBe(false)
    expect(terminal.written.at(-1)).toContain('\x1b[?25l')
    expect(dimmed()).toBe(true)
  })

  it('sends no input after the exit frame, even before xterm stops taking keys', async () => {
    const { socket, terminal } = await openTerminal('done')
    act(() => terminal.typed?.('a'))
    expect(inputs(socket)).toHaveLength(1)

    act(() => socket.onmessage?.({ data: JSON.stringify({ type: 'exit', code: 0 }) }))
    for (const data of ['ls', '\r', 'pasted text']) {
      act(() => terminal.typed?.(data))
    }
    expect(inputs(socket)).toHaveLength(1)
  })
})

describe('TerminalView open a new shell', () => {
  it('says what the button does and that it is a keyboard button in a status bar', async () => {
    const { socket } = await openTerminal('done')
    exit(socket)

    const button = newShell()
    expect(button?.tagName).toBe('BUTTON')
    expect(button?.getAttribute('data-tip')).toBe(
      "Starts a plain shell in the task's workspace. It does not start the agent.",
    )
    expect(screen.queryByRole('button', { name: 'Start a new shell' })).toBeNull()
    expect(screen.getByRole('status').tagName).toBe('P')
  })

  it('opens a new shell that takes input, with the cursor back and no bar', async () => {
    const { socket, sockets, connect } = await openTerminal('done')
    exit(socket)

    fireEvent.click(newShell() as HTMLElement)
    await flush()
    const next = sockets[1] as FakeSocket
    act(() => next.onopen?.())
    const terminal = fakes.terminals.at(-1) as FakeTerminal

    expect(connect).toHaveBeenCalledTimes(2)
    expect(terminal.options.disableStdin).not.toBe(true)
    expect(terminal.options.cursorBlink).toBe(true)
    expect(newShell()).toBeNull()
    expect(screen.queryByRole('status')).toBeNull()
    expect(dimmed()).toBe(false)
    act(() => terminal.typed?.('pwd\r'))
    expect(inputs(next)).toEqual([{ type: 'input', data: 'pwd\r' }])
  })

  it('forgets the old exit code in the new shell', async () => {
    const { socket, sockets } = await openTerminal('running')
    exit(socket, 1)
    fireEvent.click(newShell() as HTMLElement)
    await flush()
    const next = sockets[1] as FakeSocket
    act(() => next.onopen?.())

    exit(next, 0)
    expect(statusText()).toBe('The shell ended.')
  })

  it('shows the task reason again when the new shell ends while the task is done', async () => {
    const { socket, sockets } = await openTerminal('done')
    exit(socket)
    fireEvent.click(newShell() as HTMLElement)
    await flush()
    const next = sockets[1] as FakeSocket
    act(() => next.onopen?.())

    exit(next, 0)
    expect(statusText()).toBe(DONE)
  })

  it('opens one new shell on a double click', async () => {
    const { socket, connect } = await openTerminal('done')
    exit(socket)
    const button = newShell() as HTMLElement

    fireEvent.click(button)
    fireEvent.click(button)
    await flush()
    expect(connect).toHaveBeenCalledTimes(2)
  })

  it('shows the error at the top when the new shell cannot start', async () => {
    const { socket, connect } = await openTerminal('done')
    exit(socket)
    connect.mockRejectedValueOnce(new Error('No workspace.'))

    fireEvent.click(newShell() as HTMLElement)
    await flush()

    expect(statusText()).toBe('The terminal could not start. No workspace.')
    expect(screen.getByRole('button', { name: 'Start a new shell' })).toBeTruthy()
    expect(newShell()).toBeNull()
    expect(dimmed()).toBe(false)
  })
})

describe('TerminalView before the shell opens', () => {
  it('shows "Connecting." at the top while the socket opens', async () => {
    const { terminal } = await startTerminal('done')

    expect(statusText()).toBe('Connecting.')
    expect(newShell()).toBeNull()
    expect(terminal.options.disableStdin).not.toBe(true)
  })

  it('shows the error at the top when the socket closes before it opens', async () => {
    const { socket } = await startTerminal('done')
    act(() => socket.onclose?.())

    expect(statusText()).toBe('The terminal could not start. Could not connect to the terminal.')
    expect(screen.getByRole('button', { name: 'Start a new shell' })).toBeTruthy()
    expect(newShell()).toBeNull()
    expect(dimmed()).toBe(false)
  })
})
