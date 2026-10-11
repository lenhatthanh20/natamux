import { act, cleanup, render } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { RELEASE_DELAY_MS } from '../lib/gpu-renderer'
import TerminalView from './TerminalView'

type FakeTerminal = { written: string[]; disposed: boolean }
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
      constructor() {
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
      onData = listener
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
    send: () => undefined,
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

async function openTerminal() {
  const socket = fakeSocket()
  const view = render(<TerminalView connect={async () => socket as unknown as WebSocket} />)
  await flush()
  act(() => socket.onopen?.())
  const terminal = fakes.terminals.at(-1)
  if (terminal === undefined) {
    throw new Error('No terminal was made.')
  }
  return { socket, terminal, view }
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
