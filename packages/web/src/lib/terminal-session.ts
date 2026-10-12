import { ACK_EVERY_BYTES, scrollbackLines, type TerminalClientMessage } from '@natamux/core'
import { FitAddon } from '@xterm/addon-fit'
import { SearchAddon } from '@xterm/addon-search'
import { Unicode11Addon } from '@xterm/addon-unicode11'
import { WebLinksAddon } from '@xterm/addon-web-links'
import { WebglAddon } from '@xterm/addon-webgl'
import { Terminal } from '@xterm/xterm'
import { keyBelongsToShell } from './dispatch'
import { type GpuRenderer, gpuRenderer } from './gpu-renderer'
import { closeSocket } from './live'
import {
  isQueryReply,
  parseTerminalMessage,
  type TerminalConnectQuery,
  takesFocusOnOpen,
} from './terminal'
import type { CachedTerminal } from './terminal-cache'
import { type ResolvedTerminalFont, terminalFont } from './terminal-font'
import {
  MIN_CONTRAST_RATIO,
  TERMINAL_LINE_HEIGHT,
  TERMINAL_SCROLLBAR_WIDTH,
  terminalTheme,
} from './terminal-theme'
import { pageTheme, uiZoom } from './theme'

// One live terminal: xterm, its addons, its socket and its element. It lives outside React, so
// the terminal cache can keep it while no view shows it. Only TerminalView imports this file, so
// xterm stays out of the first load.

// A burst of resizes (dragging a divider) fits the terminal once, when it settles.
export const FIT_DELAY_MS = 100
export const DISPOSE_DELAY_MS = 200
// A catch-up whose `replayed` never comes ends after this, so a daemon bug never leaves a blank
// pane (spec 6).
export const CATCH_UP_TIMEOUT_MS = 10_000
// A resume up to this size is drawn in place. A larger one stays hidden until it ends, as a
// snapshot does.
export const HIDDEN_RESUME_BYTES = 64 * 1024

export type TerminalStatus = 'connecting' | 'open' | 'ended' | 'error'
export type TerminalSessionState = {
  status: TerminalStatus
  error: string | null
  scrolledUp: boolean
  // The screen is hidden while a large catch-up is written, and the view shows a skeleton.
  catchingUp: boolean
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
  readonly search: SearchAddon
  // The same object until something changes, for useSyncExternalStore.
  state(): TerminalSessionState
  subscribe(listener: () => void): () => void
  show(container: HTMLElement): void
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

// Draws with the GPU, as native terminals do. Without WebGL (old GPU, tests) or when the GPU
// drops the context, the terminal keeps or goes back to drawing with the DOM.
function loadWebgl(terminal: Terminal, onLost: () => void): WebglAddon | null {
  try {
    const webgl = new WebglAddon()
    webgl.onContextLoss(onLost)
    terminal.loadAddon(webgl)
    return webgl
  } catch {
    return null
  }
}

function fontKey(font: ResolvedTerminalFont, zoom: number): string {
  return `${font.stack}|${font.size * zoom}`
}

export function createTerminalSession(options: {
  connect: (query: TerminalConnectQuery) => Promise<WebSocket>
  onEnd?: (session: TerminalSession) => void
}): TerminalSession {
  const { connect, onEnd } = options
  let theme = pageTheme()
  const firstFont = terminalFont()
  const firstZoom = uiZoom()
  // The font the cells were measured with. Empty means "measure again".
  let measuredFont = fontKey(firstFont, firstZoom)
  const terminal = new Terminal({
    // The search addon marks its matches with decorations, and Unicode 11 widths are set
    // through terminal.unicode: both are proposed APIs.
    allowProposedApi: true,
    cursorBlink: true,
    fontFamily: firstFont.stack,
    fontSize: firstFont.size * firstZoom,
    lineHeight: TERMINAL_LINE_HEIGHT,
    minimumContrastRatio: MIN_CONTRAST_RATIO,
    overviewRuler: { width: TERMINAL_SCROLLBAR_WIDTH },
    scrollback: scrollbackLines(80),
    theme: terminalTheme(theme),
  })
  const fit = new FitAddon()
  terminal.loadAddon(fit)
  const search = new SearchAddon()
  terminal.loadAddon(search)
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

  let state: TerminalSessionState = {
    status: 'connecting',
    error: null,
    scrolledUp: false,
    catchingUp: false,
  }
  const listeners = new Set<() => void>()
  const update = (change: Partial<TerminalSessionState>) => {
    const next = { ...state, ...change }
    if (
      next.status === state.status &&
      next.error === state.error &&
      next.scrolledUp === state.scrolledUp &&
      next.catchingUp === state.catchingUp
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

  // Returns false when the socket is not open and nothing went out.
  const send = (message: TerminalClientMessage): boolean => {
    if (socket === null || socket.readyState !== WebSocket.OPEN) {
      return false
    }
    socket.send(JSON.stringify(message))
    return true
  }
  // The size that this screen sent last. It sends a size when it changes, and again when the
  // screen takes the focus: another screen of the same shell may have set its own size since.
  // Never during a catch-up: the local resizes to the snapshot size and back are not the user's.
  let sentSize = { cols: terminal.cols, rows: terminal.rows }
  const sendSize = (always = false) => {
    if (catchUp !== null) {
      return
    }
    if (always || terminal.cols !== sentSize.cols || terminal.rows !== sentSize.rows) {
      sentSize = { cols: terminal.cols, rows: terminal.rows }
      if (send({ type: 'resize', ...sentSize })) {
        claimed = true
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
  const onPageVisibility = () => sendVisible(false)

  // The textarea stays visible, so it keeps the focus and typed keys are queued, not lost.
  const hideScreen = () => {
    host.style.visibility = 'hidden'
    if (terminal.textarea !== undefined) {
      terminal.textarea.style.visibility = 'visible'
    }
    update({ catchingUp: true })
  }
  const showScreen = () => {
    host.style.visibility = ''
    if (terminal.textarea !== undefined) {
      terminal.textarea.style.visibility = ''
    }
    update({ catchingUp: false })
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
    for (const data of typed.splice(0)) {
      sendInput(data)
    }
    updateScrolled()
  }

  const beginCatchUp = (kind: CatchUp['kind']) => {
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
    document.fonts
      ?.load(`${Math.round(font.size)}px ${font.stack}`)
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
    onScreen = value
    gpu?.setVisible(value)
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

  const listen = (opened: WebSocket) => {
    socket = opened
    // Output comes in binary frames. An ArrayBuffer can be written at once, a Blob cannot.
    opened.binaryType = 'arraybuffer'
    let wasOpen = false
    opened.onopen = () => {
      wasOpen = true
      update({ status: 'open' })
      // The page may have been hidden or shown while the socket was opening.
      sendVisible(false)
      // The terminal may have been fitted while the socket was opening.
      if (claimed) {
        sendSize()
      }
    }
    opened.onmessage = (event) => {
      // typeof, not instanceof: a buffer from another realm (jsdom in tests) fails instanceof.
      if (typeof event.data !== 'string') {
        output(new Uint8Array(event.data))
        return
      }
      const message = parseTerminalMessage(event.data)
      if (message?.type === 'snapshot') {
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
        if (catchUp !== null) {
          catchUp.replayed = true
        }
        // The catch-up ends once xterm parsed the last chunk: the screen stays hidden until then,
        // and an ack means parsed. After the 10 s failure path this only acks.
        terminal.write('', () => finishCatchUp(at))
      } else if (message?.type === 'exit') {
        finishCatchUp(null)
        terminal.write('\r\n')
        update({ status: 'ended' })
        end()
      } else if (message?.type === 'error') {
        finishCatchUp(null)
        update({ status: 'error', error: message.message })
        end()
      }
    }
    opened.onclose = () => {
      if (disposed) {
        return
      }
      // A close before the open is a failed handshake, not a shell that ended.
      if (!wasOpen) {
        update({ status: 'error', error: state.error ?? 'Could not connect to the terminal.' })
      } else if (state.status !== 'error') {
        update({ status: 'ended' })
      }
      end()
    }
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
    const visible = document.visibilityState !== 'hidden'
    claimed = visible
    sentVisible = visible
    connect({ cols: terminal.cols, rows: terminal.rows, visible })
      .then((opened) => {
        if (disposed) {
          closeSocket(opened)
          return
        }
        listen(opened)
      })
      .catch((reason: unknown) => {
        if (!disposed) {
          update({
            status: 'error',
            error: reason instanceof Error ? reason.message : String(reason),
          })
          end()
        }
      })

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
        sendSize()
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
  }

  const show = (container: HTMLElement) => {
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
      clearTimeout(fitTimer)
      fitTimer = setTimeout(fitNow, FIT_DELAY_MS)
    })
    observer.observe(container)
    // Takes the settings changed while hidden, and fits once. The fit sends a resize only when
    // the pane's size changed.
    setOnScreen(true)
    if (again) {
      // The element moved; draw it fresh.
      terminal.refresh(0, terminal.rows - 1)
    }
    if (takesFocusOnOpen(document.activeElement, document.body)) {
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
      gpu?.dispose()
      terminal.dispose()
    }, DISPOSE_DELAY_MS)
  }

  document.addEventListener('visibilitychange', onPageVisibility)

  const session: TerminalSession = {
    terminal,
    search,
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
