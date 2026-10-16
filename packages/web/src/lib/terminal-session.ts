import {
  ACK_EVERY_BYTES,
  CLOSE_GONE,
  CLOSE_REFUSED,
  CLOSE_UNAUTHORIZED,
  HEARTBEAT_MS,
  scrollbackLines,
  type TerminalClientMessage,
} from '@natamux/core'
import { FitAddon } from '@xterm/addon-fit'
import { SearchAddon } from '@xterm/addon-search'
import { Unicode11Addon } from '@xterm/addon-unicode11'
import { WebLinksAddon } from '@xterm/addon-web-links'
import type { WebglAddon } from '@xterm/addon-webgl'
import { Terminal } from '@xterm/xterm'
import { keyBelongsToShell } from './dispatch'
import { ICON_FONT } from './fonts'
import { type GpuRenderer, gpuRenderer, type Renderer } from './gpu-renderer'
import { closeSocket } from './live'
import {
  isQueryReply,
  parseTerminalMessage,
  reconnectDelay,
  type TerminalConnectQuery,
  takesFocusOnOpen,
} from './terminal'
import type { CachedTerminal } from './terminal-cache'
import { type ResolvedTerminalFont, terminalFont } from './terminal-font'
import { TERMINAL_LOOK, terminalTheme } from './terminal-theme'
import { pageTheme, uiZoom } from './theme'

// One live terminal: xterm, its addons, its socket and its element. It lives outside React, so
// the terminal cache can keep it while no view shows it. Only TerminalView imports this file, so
// xterm stays out of the first load.

// A burst of resizes (dragging a divider) fits the terminal and sends its size once, when it
// settles.
export const FIT_DELAY_MS = 100
// As VS Code's terminalResizeDebouncer: rows change at once, they cost little. Columns reflow every
// wrapped line, so a buffer of this many rows or more changes them only once the pane settles.
export const REFLOW_WAIT_ROWS = 200
// A pane that changes size hides its screen until the shell drew for the new size: shown once the
// shell printed nothing for REDRAW_QUIET_MS after the resize, or REDRAW_MAX_MS after it at most.
export const REDRAW_QUIET_MS = 60
export const REDRAW_MAX_MS = 400
export const DISPOSE_DELAY_MS = 200
// A catch-up whose `replayed` never comes ends after this, so a daemon bug never leaves a blank
// pane (spec 6).
export const CATCH_UP_TIMEOUT_MS = 10_000
// A resume up to this size is drawn in place. A larger one stays hidden until it ends, as a
// snapshot does.
export const HIDDEN_RESUME_BYTES = 64 * 1024
// After these, the shell is gone or the daemon said no: a new socket would not help.
const FINAL_CLOSE_CODES = [CLOSE_UNAUTHORIZED, CLOSE_REFUSED, CLOSE_GONE]

export type TerminalStatus = 'connecting' | 'open' | 'reconnecting' | 'ended' | 'error'
export type TerminalSessionState = {
  status: TerminalStatus
  error: string | null
  scrolledUp: boolean
  // The screen is hidden while a large catch-up is written, and the view shows a skeleton.
  catchingUp: boolean
  // From the `exit` frame; null when the shell ended another way or runs.
  exitCode: number | null
}

// From `snapshot` or `resume` until xterm parsed the last chunk before `replayed` (spec 6).
type CatchUp = {
  kind: 'snapshot' | 'resume'
  bytes: number
  // `replayed` came: later frames are live output, counted, though xterm may not have parsed the
  // catch-up yet.
  replayed: boolean
  timer: ReturnType<typeof setTimeout>
}

export type TerminalSession = CachedTerminal & {
  readonly terminal: Terminal
  // Made at the first call: most terminals are never searched.
  search(): SearchAddon
  // The same object until something changes, for useSyncExternalStore.
  state(): TerminalSessionState
  subscribe(listener: () => void): () => void
  // takeFocus false keeps the keyboard where it is: of several shells side by side, only one may
  // take it, or the focus jumps from shell to shell as they show.
  show(container: HTMLElement, takeFocus?: boolean): void
  setOnScreen(onScreen: boolean): void
  applySettings(): void
}

// A terminal that is hidden, not open yet or already disposed has no size, and xterm throws
// ("reading 'dimensions'") while it measures nothing. The next resize fits it.
function safeFit(fit: FitAddon): void {
  try {
    fit.fit()
  } catch {
    // Nothing to fit yet.
  }
}

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
function loadWebgl(
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

// check() throws on a font string it cannot parse; load() then fails too, and is ignored.
function fontLoaded(spec: string): boolean {
  try {
    return document.fonts?.check(spec) === true
  } catch {
    return false
  }
}

function fontKey(font: ResolvedTerminalFont, zoom: number): string {
  return `${font.stack}|${font.size * zoom}`
}

export function createTerminalSession(options: {
  connect: (query: TerminalConnectQuery) => Promise<WebSocket>
  onEnd?: (session: TerminalSession) => void
  // For the jitter of reconnect delays; tests pass a fixed one.
  random?: () => number
}): TerminalSession {
  const { connect, onEnd, random = Math.random } = options
  let theme = pageTheme()
  const firstFont = terminalFont()
  const firstZoom = uiZoom()
  // The font the cells were measured with. Empty means "measure again".
  let measuredFont = fontKey(firstFont, firstZoom)
  const terminal = new Terminal({
    // The search addon marks its matches with decorations, and Unicode 11 widths are set
    // through terminal.unicode: both are proposed APIs.
    allowProposedApi: true,
    ...TERMINAL_LOOK,
    fontFamily: firstFont.stack,
    fontSize: firstFont.size * firstZoom,
    scrollback: scrollbackLines(80),
    theme: terminalTheme(theme),
  })
  const fit = new FitAddon()
  terminal.loadAddon(fit)
  let searchAddon: SearchAddon | null = null
  // Only web addresses become links, and they open in a new tab.
  terminal.loadAddon(
    new WebLinksAddon((event, uri) => {
      event.preventDefault()
      window.open(uri, '_blank', 'noopener,noreferrer')
    }),
  )
  // The keys of the app (F6, Cmd chords on a Mac, Ctrl+Shift chords elsewhere) are not for the
  // shell. Returning false lets the key event go on to the page, see lib/dispatch.ts.
  terminal.attachCustomKeyEventHandler((event) => keyBelongsToShell(event))
  // Emoji and wide signs take two cells, as in the daemon's screen (terminal-service.ts).
  terminal.loadAddon(new Unicode11Addon())
  terminal.unicode.activeVersion = '11'
  // xterm's own parent. It moves between a pane and the hidden holder, and FitAddon measures it,
  // so it has the classes of the box the terminal fills.
  const host = document.createElement('div')
  host.className = 'min-h-0 flex-1 overflow-hidden'
  // The fade runs once; left on, it would run again each time the element moves.
  host.addEventListener('animationend', (event) => {
    if (event.target === host) {
      host.classList.remove('hm-fade-in')
    }
  })

  let state: TerminalSessionState = {
    status: 'connecting',
    error: null,
    scrolledUp: false,
    catchingUp: false,
    exitCode: null,
  }
  const listeners = new Set<() => void>()
  const update = (change: Partial<TerminalSessionState>) => {
    const next = { ...state, ...change }
    if (
      next.status === state.status &&
      next.error === state.error &&
      next.scrolledUp === state.scrolledUp &&
      next.catchingUp === state.catchingUp &&
      next.exitCode === state.exitCode
    ) {
      return
    }
    state = next
    for (const listener of [...listeners]) {
      listener()
    }
  }

  let started = false
  let disposed = false
  let ended = false
  let onScreen = false
  let socket: WebSocket | null = null
  let gpu: GpuRenderer | null = null
  let observer: ResizeObserver | null = null
  let fitTimer: ReturnType<typeof setTimeout> | undefined
  // Set while a resize is local only: the shell gets the size once the pane settles.
  let holdSize = false
  // The screen is hidden while the pane changes size (`covered`) and while a catch-up is written
  // (`catchUpHidden`). It shows when neither holds.
  let covered = false
  let catchUpHidden = false
  // A resize went out while covered, so the shell redraws before the screen shows.
  let redrawWanted = false
  let quietTimer: ReturnType<typeof setTimeout> | undefined
  let redrawTimer: ReturnType<typeof setTimeout> | undefined
  let frame: number | null = null
  const subscriptions: { dispose(): void }[] = []
  let catchUp: CatchUp | null = null
  // Bytes of output received outside catch-ups. `replayed` sets it.
  let offset = 0
  let acked = 0
  // What the user typed during a catch-up, sent after it in order.
  const typed: string[] = []
  // Set while a real key is dispatched, so its data is never taken for a query reply.
  let fromUser = false
  let userTimer: ReturnType<typeof setTimeout> | undefined
  // This client took the shell's size (spec 2.1). One that did not sends its size first when it
  // claims, so the daemon never takes its old size from the attach.
  let claimed = false
  // The visibility the daemon knows.
  let sentVisible = true
  // The terminal came on screen during a catch-up. It claims once its own size is back.
  let claimAfterCatchUp = false
  // Reconnect (spec 5). The shell this terminal shows, from `hello`, and whether `offset` is right
  // for a resume: set by `replayed`, cleared when a catch-up starts.
  let shellId: string | null = null
  let offsetKnown = false
  // A socket of this session opened once. Before that, a close is a failed start, not a loss.
  let everOpened = false
  // A connect or a socket that is not open yet is on its way.
  let connecting = false
  let tries = 0
  let retryTimer: ReturnType<typeof setTimeout> | undefined
  let pingTimer: ReturnType<typeof setInterval> | undefined
  let pongTimer: ReturnType<typeof setTimeout> | undefined

  // Returns false when the socket is not open and nothing went out. After the end nothing goes
  // out, also keys typed before xterm stops taking them.
  const send = (message: TerminalClientMessage): boolean => {
    if (ended || socket === null || socket.readyState !== WebSocket.OPEN) {
      return false
    }
    socket.send(JSON.stringify(message))
    return true
  }
  // The size that this screen sent last. It sends a size when it changes, and again when the
  // screen takes the focus: another screen of the same shell may have set its own size since.
  // Never during a catch-up: the local resizes to the snapshot size and back are not the user's.
  let sentSize = { cols: terminal.cols, rows: terminal.rows }
  // A size that could not go out (no socket) is not taken as sent, so a fit back to the old size
  // after a reconnect sends nothing.
  const sendSize = (always = false) => {
    if (catchUp !== null) {
      return
    }
    if (always || terminal.cols !== sentSize.cols || terminal.rows !== sentSize.rows) {
      const size = { cols: terminal.cols, rows: terminal.rows }
      if (send({ type: 'resize', ...size })) {
        sentSize = size
        claimed = true
        redrawWanted ||= covered
      }
    }
  }
  const claimSize = () => sendSize(true)
  const sendInput = (data: string) => {
    if (!claimed) {
      claimSize()
    }
    send({ type: 'input', data })
  }
  // Flow control on the daemon reads it (PR 4).
  const ack = (at: number) => {
    acked = at
    send({ type: 'ack', offset: at })
  }

  // An ended shell keeps its last screen to read, but must not look like it still takes keys.
  const readOnly = () => {
    terminal.options.disableStdin = true
    terminal.options.cursorBlink = false
    terminal.write('\x1b[?25l')
  }

  // The cache drops a session whose shell ended or failed, so the next show starts fresh.
  const end = () => {
    if (!ended && !disposed) {
      ended = true
      onEnd?.(session)
    }
  }

  // Scroll state goes to React at most once per frame, and never while hidden: output of a
  // hidden terminal would re-render a view for nothing.
  const updateScrolled = () => {
    if (!onScreen || frame !== null || catchUp !== null) {
      return
    }
    frame = requestAnimationFrame(() => {
      frame = null
      const buffer = terminal.buffer.active
      update({ scrolledUp: buffer.viewportY < buffer.baseY })
    })
  }

  // claim: the terminal itself came on screen in the page, so the message carries its size and
  // takes the shell's size. The page coming back never claims (spec 2.1).
  const sendVisible = (claim: boolean) => {
    if (socket === null || socket.readyState !== WebSocket.OPEN) {
      return
    }
    const visible = onScreen && document.visibilityState !== 'hidden'
    if (claim && visible && catchUp !== null) {
      // Its size is the snapshot's until the catch-up ends.
      claimAfterCatchUp = true
    } else if (claim && visible) {
      sentVisible = true
      claimed = true
      sentSize = { cols: terminal.cols, rows: terminal.rows }
      send({ type: 'visible', visible: true, ...sentSize })
      return
    }
    if (visible !== sentVisible) {
      sentVisible = visible
      send({ type: 'visible', visible })
    }
  }
  const onPageVisibility = () => {
    sendVisible(false)
    if (pageHidden()) {
      stopHeartbeat()
      clearTimeout(retryTimer)
      retryTimer = undefined
    } else {
      startHeartbeat()
      retry()
    }
  }

  // A catch-up hides with visibility; the textarea stays visible, so it keeps the focus and typed
  // keys are queued, not lost. A cover uses opacity: the pane still takes a click, which focuses
  // the terminal (a hidden element gets no click).
  const applyScreen = () => {
    host.style.visibility = catchUpHidden ? 'hidden' : ''
    host.style.opacity = covered ? '0' : ''
    if (terminal.textarea !== undefined) {
      terminal.textarea.style.visibility = catchUpHidden ? 'visible' : ''
    }
  }
  const hideScreen = () => {
    catchUpHidden = true
    applyScreen()
    update({ catchingUp: true })
  }
  const showScreen = () => {
    catchUpHidden = false
    applyScreen()
    update({ catchingUp: false })
  }

  // The pane changes size: the frame stays, the screen goes until the shell drew for the new size,
  // so the reflow and the program's redraw are not seen.
  const stopRedrawWait = () => {
    clearTimeout(quietTimer)
    clearTimeout(redrawTimer)
    quietTimer = undefined
    redrawTimer = undefined
  }
  const cover = () => {
    stopRedrawWait()
    if (!covered) {
      covered = true
      host.classList.remove('hm-fade-in')
      applyScreen()
    }
  }
  // fade: the screen comes back in the same place; not when the element moves.
  const uncover = (fade = true) => {
    stopRedrawWait()
    redrawWanted = false
    if (!covered) {
      return
    }
    covered = false
    applyScreen()
    if (fade && onScreen && !catchUpHidden) {
      // Read in between, so the animation starts again.
      void host.offsetWidth
      host.classList.add('hm-fade-in')
    }
  }
  // Each output moves the end REDRAW_QUIET_MS later; redrawTimer is the latest end.
  const waitForQuiet = () => {
    clearTimeout(quietTimer)
    quietTimer = setTimeout(() => uncover(), REDRAW_QUIET_MS)
  }

  // at is the offset from `replayed`, or null when the catch-up failed or the shell ended. The
  // offset itself was taken when `replayed` came.
  const finishCatchUp = (at: number | null) => {
    if (disposed) {
      return
    }
    if (at !== null) {
      ack(at)
    }
    if (catchUp === null) {
      return
    }
    clearTimeout(catchUp.timer)
    const kind = catchUp.kind
    // Back to the pane's size. Its resize sends nothing: the catch-up is still on.
    if (onScreen) {
      safeFit(fit)
    }
    catchUp = null
    if (kind === 'snapshot') {
      terminal.scrollToBottom()
    }
    showScreen()
    if (claimed) {
      sendSize()
    } else {
      sentSize = { cols: terminal.cols, rows: terminal.rows }
    }
    if (claimAfterCatchUp) {
      claimAfterCatchUp = false
      sendVisible(true)
    }
    // Kept while the socket is down: the catch-up of the next socket sends them.
    if (socket?.readyState === WebSocket.OPEN) {
      for (const data of typed.splice(0)) {
        sendInput(data)
      }
    }
    updateScrolled()
  }

  const beginCatchUp = (kind: CatchUp['kind']) => {
    // Bytes of a catch-up are not counted: a socket lost now resumes with a snapshot.
    offsetKnown = false
    if (catchUp !== null) {
      clearTimeout(catchUp.timer)
    }
    const timer = setTimeout(() => finishCatchUp(null), CATCH_UP_TIMEOUT_MS)
    catchUp = { kind, bytes: 0, replayed: false, timer }
  }

  const output = (bytes: Uint8Array) => {
    if (catchUp !== null && !catchUp.replayed) {
      // Not counted: snapshot and live bytes come mixed, and `replayed` gives the offset.
      catchUp.bytes += bytes.byteLength
      if (catchUp.kind === 'resume' && catchUp.bytes > HIDDEN_RESUME_BYTES) {
        hideScreen()
      }
      terminal.write(bytes)
      return
    }
    offset += bytes.byteLength
    const at = offset
    if (redrawTimer !== undefined) {
      waitForQuiet()
    }
    terminal.write(bytes, () => {
      if (!disposed && at - acked >= ACK_EVERY_BYTES) {
        ack(at)
      }
    })
  }

  // During a snapshot the xterm keeps the snapshot's size; it fits back when the catch-up ends.
  const fitNow = () => {
    if (onScreen && catchUp?.kind !== 'snapshot') {
      safeFit(fit)
    }
  }

  // While the pane changes size: the rows follow at once, the columns too in a short buffer.
  const resizeNow = () => {
    if (!onScreen || catchUp?.kind === 'snapshot') {
      return
    }
    let size: { cols: number; rows: number } | undefined
    try {
      size = fit.proposeDimensions()
    } catch {
      // Nothing to measure yet, see safeFit.
      return
    }
    // A pane with no computed size (display: none) measures NaN, which xterm's resize throws on.
    if (size === undefined || Number.isNaN(size.cols) || Number.isNaN(size.rows)) {
      return
    }
    if (size.cols !== terminal.cols || size.rows !== terminal.rows) {
      cover()
    }
    const cols = terminal.buffer.normal.length < REFLOW_WAIT_ROWS ? size.cols : terminal.cols
    holdSize = true
    try {
      terminal.resize(cols, size.rows)
    } finally {
      holdSize = false
    }
  }

  // The pane settled: the columns catch up, and the shell gets the size once. The screen shows
  // after the shell's redraw, or at once when the shell got no new size.
  const settle = () => {
    if (!onScreen) {
      // Off screen (watchVisible): nothing to see, and no settle may come to end the cover.
      uncover(false)
      return
    }
    fitNow()
    sendSize()
    if (!covered) {
      return
    }
    if (redrawWanted) {
      redrawWanted = false
      redrawTimer = setTimeout(() => uncover(), REDRAW_MAX_MS)
      waitForQuiet()
    } else {
      uncover()
    }
  }

  // A key's data comes inside xterm's keydown listener on the textarea. The capture listener on
  // the element runs before it, and the textarea listener of the code after it (spec 6).
  const markUser = () => {
    fromUser = true
    clearTimeout(userTimer)
    // A task, never a microtask: it runs after the whole dispatch, in case a later xterm stops
    // the event before the textarea listener.
    userTimer = setTimeout(() => {
      fromUser = false
    }, 0)
  }
  const clearUser = () => {
    fromUser = false
  }

  // Setting another family first makes xterm measure the cells again, even for the same family.
  const remeasure = (stack: string) => {
    terminal.options.fontFamily = 'monospace'
    terminal.options.fontFamily = stack
  }

  // The web font may still be loading when the terminal measures its cells. Once it is loaded,
  // the terminal measures again; a hidden one at its next show, as it cannot measure now.
  const whenFontLoads = (font: ResolvedTerminalFont) => {
    const spec = `${Math.round(font.size)}px ${font.stack}`
    // Already loaded: the cells were measured with it, and measuring again would only redraw
    // every glyph, a long task when a view opens several terminals at once.
    if (fontLoaded(spec)) {
      return
    }
    document.fonts
      ?.load(spec)
      .then(() => {
        if (disposed) {
          return
        }
        if (onScreen) {
          remeasure(terminalFont().stack)
          fitNow()
        } else {
          measuredFont = ''
        }
      })
      .catch(() => undefined)
  }

  // Returns true when the font changed, so the caller fits. Call only on screen.
  const applyFont = (): boolean => {
    const font = terminalFont()
    const zoom = uiZoom()
    const key = fontKey(font, zoom)
    if (key === measuredFont) {
      return false
    }
    measuredFont = key
    remeasure(font.stack)
    terminal.options.fontSize = font.size * zoom
    whenFontLoads(font)
    return true
  }

  const applyTheme = () => {
    const next = pageTheme()
    if (next !== theme) {
      theme = next
      terminal.options.theme = terminalTheme(next)
    }
  }

  // A hidden terminal takes the font at its next show: it cannot measure cells while hidden,
  // and one change on show looks the same.
  const applySettings = () => {
    if (disposed) {
      return
    }
    applyTheme()
    if (onScreen && applyFont()) {
      fitNow()
    }
  }

  const setOnScreen = (value: boolean) => {
    if (disposed || value === onScreen) {
      return
    }
    // Before onScreen changes: a renderer taken on show then needs no fit of its own, as the fit
    // below covers it.
    gpu?.setVisible(value)
    onScreen = value
    if (value) {
      applyTheme()
      applyFont()
      fitNow()
      sendVisible(true)
      updateScrolled()
    } else {
      if (frame !== null) {
        cancelAnimationFrame(frame)
        frame = null
      }
      sendVisible(false)
    }
  }

  const pageHidden = () => document.visibilityState === 'hidden'

  const stopHeartbeat = () => {
    clearInterval(pingTimer)
    clearTimeout(pongTimer)
    pingTimer = undefined
    pongTimer = undefined
  }
  // The deadline is set before the ping goes out, so a pong can never come before it.
  const ping = () => {
    if (socket?.readyState === WebSocket.OPEN) {
      pongTimer ??= setTimeout(stale, 2 * HEARTBEAT_MS)
      send({ type: 'ping' })
    }
  }
  // Only while the page is visible: a page in the background has its timers slowed down, and a
  // pong would seem late.
  const startHeartbeat = () => {
    stopHeartbeat()
    if (!pageHidden() && socket?.readyState === WebSocket.OPEN) {
      pingTimer = setInterval(ping, HEARTBEAT_MS)
    }
  }

  // Tries again after the backoff delay. A hidden page does not: it tries when it comes back.
  const planRetry = () => {
    clearTimeout(retryTimer)
    retryTimer = undefined
    if (disposed || pageHidden()) {
      return
    }
    retryTimer = setTimeout(retry, reconnectDelay(tries, random()))
    tries += 1
  }
  // Also at once on `online` and when the page comes back, once, even with a timer planned.
  const retry = () => {
    clearTimeout(retryTimer)
    retryTimer = undefined
    if (disposed || connecting || state.status !== 'reconnecting' || pageHidden()) {
      return
    }
    open(onScreen)
  }

  // The socket closed, or went silent, while the shell may still run (spec 5). Typing queued by a
  // catch-up stays queued: the catch-up of the next socket sends it.
  const lost = () => {
    socket = null
    connecting = false
    stopHeartbeat()
    if (catchUp !== null) {
      clearTimeout(catchUp.timer)
    }
    update({ status: 'reconnecting' })
    planRetry()
  }
  // No pong in time: the connection is dead even if the browser has not noticed yet.
  const stale = () => {
    const old = socket
    if (old === null) {
      return
    }
    old.onopen = null
    old.onmessage = null
    old.onclose = null
    closeSocket(old)
    lost()
  }

  const listen = (opened: WebSocket) => {
    socket = opened
    // Output comes in binary frames. An ArrayBuffer can be written at once, a Blob cannot.
    opened.binaryType = 'arraybuffer'
    let wasOpen = false
    opened.onopen = () => {
      wasOpen = true
      everOpened = true
      connecting = false
      update({ status: 'open' })
      // The page may have been hidden or shown while the socket was opening.
      sendVisible(false)
      // The terminal may have been fitted while the socket was opening.
      if (claimed) {
        sendSize()
      }
      startHeartbeat()
    }
    opened.onmessage = (event) => {
      // typeof, not instanceof: a buffer from another realm (jsdom in tests) fails instanceof.
      if (typeof event.data !== 'string') {
        output(new Uint8Array(event.data))
        return
      }
      const message = parseTerminalMessage(event.data)
      if (message?.type === 'hello') {
        shellId = message.shellId
        // The daemon answered: the next loss starts the backoff from the first delay.
        tries = 0
      } else if (message?.type === 'pong') {
        clearTimeout(pongTimer)
        pongTimer = undefined
      } else if (message?.type === 'snapshot') {
        beginCatchUp('snapshot')
        hideScreen()
        const { cols, rows } = message
        // In xterm's write queue: the bytes before are parsed first, the snapshot after. The
        // resize is local only (sendSize sends nothing during a catch-up).
        terminal.write('', () => {
          if (!disposed) {
            terminal.reset()
            terminal.resize(cols, rows)
          }
        })
      } else if (message?.type === 'resume') {
        beginCatchUp('resume')
      } else if (message?.type === 'replayed') {
        const at = message.offset
        // The offset now, in the order of the frames: live output that comes before xterm parsed
        // the catch-up counts from here. Left uncounted, the client would ack too little forever,
        // and the daemon would never catch it up again once it is behind.
        offset = at
        offsetKnown = true
        if (catchUp !== null) {
          catchUp.replayed = true
        }
        // The catch-up ends once xterm parsed the last chunk: the screen stays hidden until then,
        // and an ack means parsed. After the 10 s failure path this only acks. A catch-up of a
        // later socket is not this one's to end.
        const current = catchUp
        terminal.write('', () => {
          if (catchUp === current) {
            finishCatchUp(at)
          }
        })
      } else if (message?.type === 'exit') {
        finishCatchUp(null)
        terminal.write('\r\n')
        readOnly()
        update({ status: 'ended', exitCode: message.code })
        end()
      } else if (message?.type === 'error') {
        finishCatchUp(null)
        update({ status: 'error', error: message.message })
        end()
      }
    }
    opened.onclose = (event) => {
      if (disposed || socket !== opened) {
        return
      }
      // After `exit` or `error` the session is over; after a final code a retry would not help.
      if (!ended && everOpened && !FINAL_CLOSE_CODES.includes(event.code)) {
        lost()
        return
      }
      socket = null
      connecting = false
      stopHeartbeat()
      if (ended) {
        return
      }
      finishCatchUp(null)
      // A close before the open is a failed handshake, not a shell that ended.
      if (wasOpen) {
        readOnly()
        update({ status: 'ended' })
      } else {
        update({ status: 'error', error: state.error ?? 'Could not connect to the terminal.' })
      }
      end()
    }
  }

  // The first socket asks for the shell as it is, and starts it when none runs. A later one
  // resumes the shell it had: from its offset when known, else with a snapshot (spec 5). A resume
  // never takes the shell's size; its next claim sends the size.
  const open = (onScreenNow: boolean) => {
    connecting = true
    const visible = onScreenNow && !pageHidden()
    claimed = visible && shellId === null
    sentVisible = visible
    const query: TerminalConnectQuery = { cols: terminal.cols, rows: terminal.rows, visible }
    if (shellId !== null) {
      query.shellId = shellId
      if (offsetKnown) {
        query.offset = offset
      }
    }
    connect(query)
      .then((opened) => {
        if (disposed) {
          closeSocket(opened)
          return
        }
        listen(opened)
      })
      .catch((reason: unknown) => {
        if (disposed) {
          return
        }
        connecting = false
        if (everOpened) {
          planRetry()
          return
        }
        update({
          status: 'error',
          error: reason instanceof Error ? reason.message : String(reason),
        })
        end()
      })
  }

  // The first show opens xterm and connects, at the size of the pane it shows in.
  const start = () => {
    terminal.open(host)
    // After open(), so the textarea listener runs after xterm's own one.
    terminal.element?.addEventListener('keydown', markUser, true)
    terminal.textarea?.addEventListener('keydown', clearUser)
    const load = (onLost: () => void) => loadWebgl(terminal, onLost)
    // A renderer switch can change the cell size, so it fits again. A hidden terminal that gives
    // its renderer back fits at its next show instead.
    gpu = gpuRenderer(load, { onSwitch: fitNow })
    safeFit(fit)
    terminal.options.scrollback = scrollbackLines(terminal.cols)
    sentSize = { cols: terminal.cols, rows: terminal.rows }
    terminal.textarea?.addEventListener('focus', claimSize)

    // start() runs from show(), so the terminal is about to be on screen. With the page hidden,
    // it does not take the shell's size (spec 2.1).
    open(true)

    subscriptions.push(
      terminal.onData((data) => {
        if (catchUp === null) {
          sendInput(data)
        } else if (fromUser || !isQueryReply(data)) {
          // Sent after the catch-up. A reply to a replayed query is dropped: it is not the user's,
          // and the program asked long ago.
          typed.push(data)
        }
      }),
      terminal.onResize(() => {
        // The history keeps the same memory at any width, as the daemon's screen does.
        terminal.options.scrollback = scrollbackLines(terminal.cols)
        if (!holdSize) {
          sendSize()
        }
      }),
      terminal.onScroll(updateScrolled),
      terminal.onWriteParsed(updateScrolled),
    )
    whenFontLoads(firstFont)
  }

  const stopObserving = () => {
    observer?.disconnect()
    observer = null
    clearTimeout(fitTimer)
    fitTimer = undefined
    // Without the observer the pane never settles, so the screen must not wait for it.
    uncover(false)
  }

  const show = (container: HTMLElement, takeFocus = true) => {
    if (disposed) {
      return
    }
    container.append(host)
    const again = started
    if (!started) {
      started = true
      start()
    }
    stopObserving()
    observer = new ResizeObserver(() => {
      resizeNow()
      clearTimeout(fitTimer)
      fitTimer = setTimeout(settle, FIT_DELAY_MS)
    })
    observer.observe(container)
    // Takes the settings changed while hidden, and fits once. The fit sends a resize only when
    // the pane's size changed.
    setOnScreen(true)
    if (again) {
      // The element moved; draw it fresh.
      terminal.refresh(0, terminal.rows - 1)
    }
    if (takeFocus && takesFocusOnOpen(document.activeElement, document.body)) {
      terminal.focus()
    }
  }

  // Nothing fits while hidden: a fit of a box with no size would send a tiny resize and make the
  // program redraw.
  const hide = (holder: HTMLElement) => {
    if (disposed) {
      return
    }
    stopObserving()
    setOnScreen(false)
    holder.append(host)
  }

  const dispose = () => {
    if (disposed) {
      return
    }
    // Freed with xterm below. Hidden first, it would only start its give-back timer.
    const disposing = gpu
    gpu = null
    setOnScreen(false)
    disposed = true
    stopObserving()
    for (const subscription of subscriptions.splice(0)) {
      subscription.dispose()
    }
    terminal.textarea?.removeEventListener('focus', claimSize)
    terminal.element?.removeEventListener('keydown', markUser, true)
    terminal.textarea?.removeEventListener('keydown', clearUser)
    document.removeEventListener('visibilitychange', onPageVisibility)
    window.removeEventListener('online', retry)
    document.fonts?.removeEventListener('loadingdone', onFontsLoaded)
    stopHeartbeat()
    clearTimeout(retryTimer)
    if (catchUp !== null) {
      clearTimeout(catchUp.timer)
      catchUp = null
    }
    clearTimeout(userTimer)
    if (socket !== null) {
      closeSocket(socket)
    }
    listeners.clear()
    // xterm still has a frame of work queued that reads its size ("reading 'dimensions'"), so it
    // is taken off the page now and disposed of a moment later.
    host.remove()
    setTimeout(() => {
      disposing?.dispose()
      terminal.dispose()
    }, DISPOSE_DELAY_MS)
  }

  // The icon font loads when the first icon is drawn (its unicode-range in styles.css). By then
  // the GPU renderer has cached that glyph as an empty box, so the glyphs are drawn again.
  const onFontsLoaded = (event: FontFaceSetLoadEvent) => {
    if (event.fontfaces.some((face) => face.family.includes(ICON_FONT))) {
      terminal.clearTextureAtlas()
    }
  }

  document.addEventListener('visibilitychange', onPageVisibility)
  window.addEventListener('online', retry)
  document.fonts?.addEventListener('loadingdone', onFontsLoaded)

  const session: TerminalSession = {
    terminal,
    search() {
      if (searchAddon === null) {
        searchAddon = new SearchAddon()
        terminal.loadAddon(searchAddon)
      }
      return searchAddon
    },
    state: () => state,
    subscribe(listener) {
      listeners.add(listener)
      return () => {
        listeners.delete(listener)
      }
    },
    show,
    hide,
    setOnScreen,
    applySettings,
    dispose,
  }
  return session
}
