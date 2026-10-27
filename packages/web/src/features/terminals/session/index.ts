import { scrollbackLines } from '@natamux/core'
import { SearchAddon } from '@xterm/addon-search'
import { ICON_FONT } from '../../../lib/fonts'
import { closeSocket } from '../../../lib/live'
import { pageTheme, uiZoom } from '../../../lib/theme'
import { gpuRenderer } from '../gpu-renderer'
import { isQueryReply, type TerminalConnectQuery, takesFocusOnOpen } from '../terminal'
import { terminalFont } from '../terminal-font'
import { createCatchUp } from './catch-up'
import { createConnection, createSender } from './connection'
import { DISPOSE_DELAY_MS, FIT_DELAY_MS } from './constants'
import { createLayout, createXterm, fontKey, safeFit } from './layout'
import type { SessionCore, TerminalSession, TerminalSessionState } from './types'
import { loadWebgl } from './webgl'

// One live terminal: xterm, its addons, its socket and its element. It lives outside React, so
// the terminal cache can keep it while no view shows it. Only TerminalView imports this file, so
// xterm stays out of the first load.

export function createTerminalSession(options: {
  connect: (query: TerminalConnectQuery) => Promise<WebSocket>
  onEnd?: (session: TerminalSession) => void
  // For the jitter of reconnect delays; tests pass a fixed one.
  random?: () => number
}): TerminalSession {
  const { connect, onEnd, random = Math.random } = options
  const theme = pageTheme()
  const firstFont = terminalFont()
  const firstZoom = uiZoom()
  const measuredFont = fontKey(firstFont, firstZoom)
  const { terminal, fit, host } = createXterm(firstFont, firstZoom, theme)
  let searchAddon: SearchAddon | null = null

  const s: SessionCore = {
    terminal,
    fit,
    host,
    state: {
      status: 'connecting',
      error: null,
      scrolledUp: false,
      catchingUp: false,
      exitCode: null,
      pausedMinutes: null,
    },
    theme,
    measuredFont,
    disposed: false,
    ended: false,
    onScreen: false,
    socket: null,
    gpu: null,
    holdSize: false,
    covered: false,
    redrawWanted: false,
    redrawTimer: undefined,
    catchUp: null,
    offset: 0,
    acked: 0,
    typed: [],
    claimed: false,
    sentVisible: true,
    claimAfterCatchUp: false,
    sentSize: { cols: terminal.cols, rows: terminal.rows },
    offsetKnown: false,
  }
  const listeners = new Set<() => void>()
  const update = (change: Partial<TerminalSessionState>) => {
    const state = s.state
    const next = { ...state, ...change }
    if (
      next.status === state.status &&
      next.error === state.error &&
      next.scrolledUp === state.scrolledUp &&
      next.catchingUp === state.catchingUp &&
      next.exitCode === state.exitCode &&
      next.pausedMinutes === state.pausedMinutes
    ) {
      return
    }
    s.state = next
    for (const listener of [...listeners]) {
      listener()
    }
  }

  let started = false
  let observer: ResizeObserver | null = null
  let fitTimer: ReturnType<typeof setTimeout> | undefined
  const subscriptions: { dispose(): void }[] = []
  // Set while a real key is dispatched, so its data is never taken for a query reply.
  let fromUser = false
  let userTimer: ReturnType<typeof setTimeout> | undefined

  // The cache drops a session whose shell ended or failed, so the next show starts fresh.
  const end = () => {
    if (!s.ended && !s.disposed) {
      s.ended = true
      onEnd?.(session)
    }
  }

  const sender = createSender(s)
  const { sendSize, claimSize, sendInput } = sender
  const layout = createLayout(s, { update, ...sender })
  const {
    updateScrolled,
    uncover,
    fitNow,
    resizeNow,
    settle,
    whenFontLoads,
    applySettings,
    setOnScreen,
  } = layout
  const catchUps = createCatchUp(s, { ...sender, ...layout })
  const { open, retry, cancelRetry, wake, onPageVisibility, stopHeartbeat } = createConnection(s, {
    connect,
    random,
    update,
    end,
    ...sender,
    ...layout,
    ...catchUps,
  })
  // Coming on screen wakes a paused terminal.
  const onScreenNow = (onScreen: boolean) => {
    setOnScreen(onScreen)
    if (onScreen) {
      wake()
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

  // The first show opens xterm and connects, at the size of the pane it shows in.
  const start = () => {
    terminal.open(host)
    // After open(), so the textarea listener runs after xterm's own one.
    terminal.element?.addEventListener('keydown', markUser, true)
    terminal.textarea?.addEventListener('keydown', clearUser)
    const load = (onLost: () => void) => loadWebgl(terminal, onLost)
    // A renderer switch can change the cell size, so it fits again. A hidden terminal that gives
    // its renderer back fits at its next show instead.
    s.gpu = gpuRenderer(load, { onSwitch: fitNow })
    safeFit(fit)
    terminal.options.scrollback = scrollbackLines(terminal.cols)
    s.sentSize = { cols: terminal.cols, rows: terminal.rows }
    terminal.textarea?.addEventListener('focus', claimSize)

    // start() runs from show(), so the terminal is about to be on screen. With the page hidden,
    // it does not take the shell's size (spec 2.1).
    open(true)

    subscriptions.push(
      terminal.onData((data) => {
        if (s.state.status === 'paused') {
          // Sent after the catch-up of the socket that brings the agent back.
          s.typed.push(data)
          wake()
        } else if (s.catchUp === null) {
          sendInput(data)
        } else if (fromUser || !isQueryReply(data)) {
          // Sent after the catch-up. A reply to a replayed query is dropped: it is not the user's,
          // and the program asked long ago.
          s.typed.push(data)
        }
      }),
      terminal.onResize(() => {
        // The history keeps the same memory at any width, as the daemon's screen does.
        terminal.options.scrollback = scrollbackLines(terminal.cols)
        if (!s.holdSize) {
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
    if (s.disposed) {
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
    onScreenNow(true)
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
    if (s.disposed) {
      return
    }
    stopObserving()
    setOnScreen(false)
    holder.append(host)
  }

  const dispose = () => {
    if (s.disposed) {
      return
    }
    // Freed with xterm below. Hidden first, it would only start its give-back timer.
    const disposing = s.gpu
    s.gpu = null
    setOnScreen(false)
    s.disposed = true
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
    cancelRetry()
    if (s.catchUp !== null) {
      clearTimeout(s.catchUp.timer)
      s.catchUp = null
    }
    clearTimeout(userTimer)
    if (s.socket !== null) {
      closeSocket(s.socket)
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
    state: () => s.state,
    subscribe(listener) {
      listeners.add(listener)
      return () => {
        listeners.delete(listener)
      }
    },
    show,
    hide,
    setOnScreen: onScreenNow,
    applySettings,
    dispose,
  }
  return session
}
