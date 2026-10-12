// Stand-ins for xterm and its addons, for tests: jsdom has no canvas. A test file mocks each
// package with this file, for example:
//   vi.mock('@xterm/xterm', async () => ({
//     Terminal: (await import('../lib/xterm-fake.fixture')).FakeTerminal,
//   }))
// The fakes register themselves in `fakes`, so a test can reach the ones the code made.

export type FakeEvent = 'data' | 'resize' | 'scroll' | 'writeParsed'
type Listener = (value: unknown) => void
type Activates = { activate?: (terminal: FakeTerminal) => void }

export const fakes = {
  terminals: [] as FakeTerminal[],
  fits: [] as FakeFitAddon[],
  searches: [] as FakeSearchAddon[],
  webgl: [] as FakeWebglAddon[],
}

export function resetFakes(): void {
  fakes.terminals.length = 0
  fakes.fits.length = 0
  fakes.searches.length = 0
  fakes.webgl.length = 0
}

const noop = { dispose: () => undefined }

export class FakeTerminal {
  options: Record<string, unknown>
  cols = 80
  rows = 24
  // The size the next fit gives, as if the pane had that room.
  fitSize = { cols: 80, rows: 24 }
  unicode = { activeVersion: '' }
  buffer = { active: { viewportY: 0, baseY: 0 } }
  element: HTMLElement | undefined = undefined
  textarea: HTMLTextAreaElement | undefined = undefined
  // Each write as text (bytes decoded), and as the code passed it.
  written: string[] = []
  writes: (string | Uint8Array)[] = []
  // Write callbacks that wait for parse().
  callbacks: (() => void)[] = []
  resets = 0
  // What xterm's own keydown listener emits as data, or null for a key that gives none.
  keyData: string | null = null
  refreshes = 0
  focuses = 0
  disposed = false
  private readonly listeners = new Map<FakeEvent, Set<Listener>>()

  constructor(options: Record<string, unknown> = {}) {
    this.options = { ...options }
    fakes.terminals.push(this)
  }

  loadAddon(addon: Activates): void {
    addon.activate?.(this)
  }

  attachCustomKeyEventHandler = () => undefined

  open(parent: HTMLElement): void {
    const element = document.createElement('div')
    element.className = 'xterm'
    const textarea = document.createElement('textarea')
    element.append(textarea)
    parent.append(element)
    this.element = element
    this.textarea = textarea
    // Stands for xterm's listener, which it adds in open(), before any listener of the code.
    textarea.addEventListener('keydown', () => {
      if (this.keyData !== null) {
        this.fire('data', this.keyData)
      }
    })
  }

  write(data: string | Uint8Array, callback?: () => void): void {
    this.written.push(typeof data === 'string' ? data : new TextDecoder().decode(data))
    this.writes.push(data)
    if (callback !== undefined) {
      this.callbacks.push(callback)
    }
  }

  // What xterm does once it parsed the writes so far: it runs their callbacks in order.
  parse(): void {
    while (this.callbacks.length > 0) {
      this.callbacks.shift()?.()
    }
  }

  reset(): void {
    this.resets += 1
  }

  refresh(): void {
    this.refreshes += 1
  }

  focus(): void {
    this.focuses += 1
  }

  scrollToBottom(): void {
    this.buffer.active = { ...this.buffer.active, viewportY: this.buffer.active.baseY }
  }

  resize(cols: number, rows: number): void {
    if (cols === this.cols && rows === this.rows) {
      return
    }
    this.cols = cols
    this.rows = rows
    this.fire('resize', { cols, rows })
  }

  onData(listener: Listener) {
    return this.listen('data', listener)
  }

  onResize(listener: Listener) {
    return this.listen('resize', listener)
  }

  onScroll(listener: Listener) {
    return this.listen('scroll', listener)
  }

  onWriteParsed(listener: Listener) {
    return this.listen('writeParsed', listener)
  }

  // What xterm would emit: typed data, a new size, a scroll, a parsed write.
  fire(event: FakeEvent, value?: unknown): void {
    for (const listener of [...(this.listeners.get(event) ?? [])]) {
      listener(value)
    }
  }

  // How many listeners an event has now, to check that dispose removed them.
  listenerCount(event: FakeEvent): number {
    return this.listeners.get(event)?.size ?? 0
  }

  dispose(): void {
    this.disposed = true
  }

  private listen(event: FakeEvent, listener: Listener) {
    const set = this.listeners.get(event) ?? new Set<Listener>()
    this.listeners.set(event, set)
    set.add(listener)
    return { dispose: () => set.delete(listener) }
  }
}

export class FakeFitAddon {
  fits = 0
  private terminal: FakeTerminal | null = null

  constructor() {
    fakes.fits.push(this)
  }

  activate(terminal: FakeTerminal): void {
    this.terminal = terminal
  }

  // As the real addon, a terminal that is not open has nothing to measure.
  fit(): void {
    this.fits += 1
    const terminal = this.terminal
    if (terminal?.element !== undefined) {
      terminal.resize(terminal.fitSize.cols, terminal.fitSize.rows)
    }
  }

  dispose = () => undefined
}

export class FakeSearchAddon {
  constructor() {
    fakes.searches.push(this)
  }

  activate = () => undefined
  findNext(): boolean {
    return false
  }
  findPrevious(): boolean {
    return false
  }
  clearDecorations = () => undefined
  onDidChangeResults() {
    return noop
  }
  dispose = () => undefined
}

// Unicode 11 and web links: nothing to fake but the addon shape.
export class FakeAddon {
  activate = () => undefined
  dispose = () => undefined
}

export class FakeWebglAddon {
  disposed = false
  // Calls the code's context-loss listener, as a GPU that drops the context.
  loseContext: () => void = () => undefined

  constructor() {
    fakes.webgl.push(this)
  }

  activate = () => undefined

  onContextLoss(listener: () => void) {
    this.loseContext = listener
    return noop
  }

  dispose(): void {
    this.disposed = true
  }
}

export type FakeSocket = ReturnType<typeof fakeSocket>

// A WebSocket that a test drives from the daemon's side: open, message, bytes, drop.
// Pings are counted in `pings`, not kept in `sent`, and answered at once while `answerPings` is
// set, as the daemon does: tests that do not look at the heartbeat need not know about it.
export function fakeSocket() {
  const socket = {
    readyState: 0,
    closed: false,
    binaryType: 'blob' as string,
    sent: [] as unknown[],
    pings: 0,
    answerPings: true,
    onopen: null as null | (() => void),
    onmessage: null as null | ((event: { data: string | ArrayBuffer }) => void),
    onclose: null as null | ((event: { code: number }) => void),
    onerror: null as null | (() => void),
    send(data: string) {
      const message = JSON.parse(data)
      if (message.type === 'ping') {
        socket.pings += 1
        if (socket.answerPings) {
          socket.message({ type: 'pong' })
        }
        return
      }
      socket.sent.push(message)
    },
    close() {
      socket.closed = true
      socket.readyState = 3
    },
    open() {
      socket.readyState = 1
      socket.onopen?.()
    },
    message(message: object) {
      socket.onmessage?.({ data: JSON.stringify(message) })
    },
    // A binary frame, as the socket gives it with binaryType 'arraybuffer': a fresh buffer.
    bytes(data: string | Uint8Array) {
      const view = typeof data === 'string' ? new TextEncoder().encode(data) : data
      socket.onmessage?.({ data: new Uint8Array(view).buffer })
    },
    // The connection ends without the client asking: 1006 when the network drops it.
    drop(code = 1006) {
      socket.readyState = 3
      socket.onclose?.({ code })
    },
    asWebSocket(): WebSocket {
      return socket as unknown as WebSocket
    },
  }
  return socket
}
