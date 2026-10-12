import { scrollbackLines, type TerminalSize } from '@natamux/core'
import { FitAddon } from '@xterm/addon-fit'
import { SearchAddon } from '@xterm/addon-search'
import { Unicode11Addon } from '@xterm/addon-unicode11'
import { WebLinksAddon } from '@xterm/addon-web-links'
import { WebglAddon } from '@xterm/addon-webgl'
import { Terminal } from '@xterm/xterm'
import { keyBelongsToShell } from './dispatch'
import { type GpuRenderer, gpuRenderer } from './gpu-renderer'
import { closeSocket } from './live'
import { parseTerminalMessage, takesFocusOnOpen } from './terminal'
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

export type TerminalStatus = 'connecting' | 'open' | 'ended' | 'error'
export type TerminalSessionState = {
  status: TerminalStatus
  error: string | null
  scrolledUp: boolean
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
  connect: (size: TerminalSize) => Promise<WebSocket>
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

  let state: TerminalSessionState = { status: 'connecting', error: null, scrolledUp: false }
  const listeners = new Set<() => void>()
  const update = (change: Partial<TerminalSessionState>) => {
    const next = { ...state, ...change }
    if (
      next.status === state.status &&
      next.error === state.error &&
      next.scrolledUp === state.scrolledUp
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

  const send = (message: object) => {
    if (socket?.readyState === WebSocket.OPEN) {
      socket.send(JSON.stringify(message))
    }
  }
  // The size that this screen sent last. It sends a size when it changes, and again when the
  // screen takes the focus: another screen of the same shell may have set its own size since.
  let sentSize = { cols: terminal.cols, rows: terminal.rows }
  const sendSize = (always = false) => {
    if (always || terminal.cols !== sentSize.cols || terminal.rows !== sentSize.rows) {
      sentSize = { cols: terminal.cols, rows: terminal.rows }
      send({ type: 'resize', ...sentSize })
    }
  }
  const claimSize = () => sendSize(true)

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
    if (!onScreen || frame !== null) {
      return
    }
    frame = requestAnimationFrame(() => {
      frame = null
      const buffer = terminal.buffer.active
      update({ scrolledUp: buffer.viewportY < buffer.baseY })
    })
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
          safeFit(fit)
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
      safeFit(fit)
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
      safeFit(fit)
      updateScrolled()
    } else if (frame !== null) {
      cancelAnimationFrame(frame)
      frame = null
    }
  }

  const listen = (opened: WebSocket) => {
    socket = opened
    let wasOpen = false
    opened.onopen = () => {
      wasOpen = true
      update({ status: 'open' })
      // The terminal may have been fitted while the socket was opening.
      sendSize()
    }
    opened.onmessage = (event) => {
      const message = parseTerminalMessage(String(event.data))
      if (message?.type === 'output') {
        terminal.write(message.data)
      } else if (message?.type === 'exit') {
        terminal.write('\r\n')
        update({ status: 'ended' })
        end()
      } else if (message?.type === 'error') {
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
    const load = (onLost: () => void) => loadWebgl(terminal, onLost)
    // A renderer switch can change the cell size, so it fits again. A hidden terminal that gives
    // its renderer back fits at its next show instead.
    const onSwitch = () => {
      if (onScreen) {
        safeFit(fit)
      }
    }
    gpu = gpuRenderer(load, { onSwitch })
    safeFit(fit)
    terminal.options.scrollback = scrollbackLines(terminal.cols)
    sentSize = { cols: terminal.cols, rows: terminal.rows }
    terminal.textarea?.addEventListener('focus', claimSize)

    connect({ cols: terminal.cols, rows: terminal.rows })
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
      terminal.onData((data) => send({ type: 'input', data })),
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
      fitTimer = setTimeout(() => {
        if (onScreen) {
          safeFit(fit)
        }
      }, FIT_DELAY_MS)
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
