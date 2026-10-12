import { scrollbackLines, type TerminalSize } from '@natamux/core'
import { FitAddon } from '@xterm/addon-fit'
import { SearchAddon } from '@xterm/addon-search'
import { Unicode11Addon } from '@xterm/addon-unicode11'
import { WebLinksAddon } from '@xterm/addon-web-links'
import { WebglAddon } from '@xterm/addon-webgl'
import { Terminal } from '@xterm/xterm'
import '@xterm/xterm/css/xterm.css'
import { ArrowDown } from 'lucide-react'
import { useEffect, useRef, useState } from 'react'
import { useAppEvent } from '../lib/app-events'
import { keyBelongsToShell } from '../lib/dispatch'
import { gpuRenderer, watchVisible } from '../lib/gpu-renderer'
import { closeSocket } from '../lib/live'
import { parseTerminalMessage, takesFocusOnOpen } from '../lib/terminal'
import { terminalFont, useTerminalFont } from '../lib/terminal-font'
import {
  MIN_CONTRAST_RATIO,
  TERMINAL_LINE_HEIGHT,
  TERMINAL_SCROLLBAR_WIDTH,
  terminalTheme,
} from '../lib/terminal-theme'
import { uiZoom, useResolvedTheme } from '../lib/theme'
import { useUiSize } from '../lib/ui-size'
import { TerminalFind } from './TerminalFind'
import { Button } from './ui/Button'

type Status = 'connecting' | 'open' | 'ended' | 'error'

// A burst of resizes (dragging a divider) fits the terminal once, when it settles.
const FIT_DELAY_MS = 100
const DISPOSE_DELAY_MS = 200

type TerminalViewProps = {
  connect: (size: TerminalSize) => Promise<WebSocket>
  // A new number takes the keyboard focus into the terminal, for "Answer in terminal".
  focusToken?: number
  // False keeps the keyboard where it is when the terminal opens. Of several shells side by side,
  // only one may take it, or the focus jumps from shell to shell as they open.
  focusOnOpen?: boolean
}

// A terminal that is hidden, not open yet or already disposed has no size, and xterm throws
// ("reading 'dimensions'") while it measures nothing. The next resize fits it.
function safeFit(fit: FitAddon | null): void {
  try {
    fit?.fit()
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

const STATUS_TEXT: Record<Status, string> = {
  connecting: 'Connecting.',
  open: 'Connected.',
  ended: 'The shell ended.',
  error: 'The terminal could not start.',
}

// A shell in the task's workspace. The default export is what ChangesView loads on demand.
export default function TerminalView({
  connect,
  focusToken,
  focusOnOpen = true,
}: TerminalViewProps) {
  const container = useRef<HTMLDivElement>(null)
  const connectRef = useRef(connect)
  connectRef.current = connect
  const focusOnOpenRef = useRef(focusOnOpen)
  focusOnOpenRef.current = focusOnOpen
  const [status, setStatus] = useState<Status>('connecting')
  const [error, setError] = useState<string | null>(null)
  const [attempt, setAttempt] = useState(0)
  const [search, setSearch] = useState<SearchAddon | null>(null)
  const [findOpen, setFindOpen] = useState(false)
  const [scrolledUp, setScrolledUp] = useState(false)
  const theme = useResolvedTheme()
  const themeRef = useRef(theme)
  themeRef.current = theme
  const terminalRef = useRef<Terminal | null>(null)
  const fitRef = useRef<FitAddon | null>(null)
  const uiSize = useUiSize()
  const font = useTerminalFont()

  // biome-ignore lint/correctness/useExhaustiveDependencies: attempt starts a new shell on purpose.
  useEffect(() => {
    const element = container.current
    if (!element) {
      return
    }
    const start = terminalFont()
    const terminal = new Terminal({
      // The search addon marks its matches with decorations, and Unicode 11 widths are set
      // through terminal.unicode: both are proposed APIs.
      allowProposedApi: true,
      cursorBlink: true,
      fontFamily: start.stack,
      fontSize: start.size * uiZoom(),
      lineHeight: TERMINAL_LINE_HEIGHT,
      minimumContrastRatio: MIN_CONTRAST_RATIO,
      overviewRuler: { width: TERMINAL_SCROLLBAR_WIDTH },
      scrollback: scrollbackLines(80),
      theme: terminalTheme(themeRef.current),
    })
    terminalRef.current = terminal
    const fit = new FitAddon()
    terminal.loadAddon(fit)
    fitRef.current = fit
    const searchAddon = new SearchAddon()
    terminal.loadAddon(searchAddon)
    setSearch(searchAddon)
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
    terminal.open(element)
    const gpu = gpuRenderer((onLost) => loadWebgl(terminal, onLost))
    const stopWatching = watchVisible(element, gpu.setVisible)
    safeFit(fit)
    terminal.options.scrollback = scrollbackLines(terminal.cols)

    let socket: WebSocket | null = null
    let disposed = false
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
    terminal.textarea?.addEventListener('focus', claimSize)

    setStatus('connecting')
    setError(null)
    connectRef
      .current({ cols: terminal.cols, rows: terminal.rows })
      .then((opened) => {
        if (disposed) {
          closeSocket(opened)
          return
        }
        socket = opened
        let wasOpen = false
        opened.onopen = () => {
          wasOpen = true
          setStatus('open')
          // The terminal may have been fitted while the socket was opening.
          sendSize()
        }
        opened.onmessage = (event) => {
          const message = parseTerminalMessage(String(event.data))
          if (message?.type === 'output') {
            terminal.write(message.data)
          } else if (message?.type === 'exit') {
            terminal.write('\r\n')
            setStatus('ended')
          } else if (message?.type === 'error') {
            setError(message.message)
            setStatus('error')
          }
        }
        opened.onclose = () => {
          if (disposed) {
            return
          }
          // A close before the open is a failed handshake, not a shell that ended.
          if (!wasOpen) {
            setError((current) => current ?? 'Could not connect to the terminal.')
            setStatus('error')
            return
          }
          setStatus((current) => (current === 'error' ? current : 'ended'))
        }
      })
      .catch((reason: unknown) => {
        if (!disposed) {
          setError(reason instanceof Error ? reason.message : String(reason))
          setStatus('error')
        }
      })

    const input = terminal.onData((data) => send({ type: 'input', data }))
    const resized = terminal.onResize(() => {
      // The history keeps the same memory at any width, as the daemon's screen does.
      terminal.options.scrollback = scrollbackLines(terminal.cols)
      sendSize()
    })
    const updateScrolled = () => {
      const buffer = terminal.buffer.active
      setScrolledUp(buffer.viewportY < buffer.baseY)
    }
    const scrolled = terminal.onScroll(updateScrolled)
    const written = terminal.onWriteParsed(updateScrolled)

    let fitTimer: ReturnType<typeof setTimeout> | undefined
    const observer = new ResizeObserver(() => {
      clearTimeout(fitTimer)
      fitTimer = setTimeout(() => {
        safeFit(fit)
      }, FIT_DELAY_MS)
    })
    observer.observe(element)
    if (focusOnOpenRef.current && takesFocusOnOpen(document.activeElement, document.body)) {
      terminal.focus()
    }

    // The web font may still be loading when the terminal measures its cells. Setting the font
    // again makes it measure with the loaded one.
    document.fonts
      ?.load(`${Math.round(start.size)}px ${start.stack}`)
      .then(() => {
        if (!disposed) {
          terminal.options.fontFamily = 'monospace'
          terminal.options.fontFamily = terminalFont().stack
          safeFit(fit)
        }
      })
      .catch(() => undefined)

    return () => {
      disposed = true
      clearTimeout(fitTimer)
      observer.disconnect()
      stopWatching()
      terminal.textarea?.removeEventListener('focus', claimSize)
      input.dispose()
      resized.dispose()
      scrolled.dispose()
      written.dispose()
      if (socket !== null) {
        closeSocket(socket)
      }
      // xterm still has a frame of work queued that reads its size ("reading 'dimensions'"), so it
      // is taken off the page now and disposed of a moment later.
      terminal.element?.remove()
      setTimeout(() => {
        gpu.dispose()
        terminal.dispose()
      }, DISPOSE_DELAY_MS)
      terminalRef.current = null
      fitRef.current = null
      setSearch(null)
      setFindOpen(false)
    }
  }, [attempt])

  useEffect(() => {
    if (terminalRef.current) {
      terminalRef.current.options.theme = terminalTheme(theme)
    }
  }, [theme])

  // The UI size and the terminal font change the cells, and then the number of columns.
  // biome-ignore lint/correctness/useExhaustiveDependencies: uiSize is a trigger; uiZoom() reads the page.
  useEffect(() => {
    const terminal = terminalRef.current
    if (terminal) {
      terminal.options.fontFamily = font.stack
      terminal.options.fontSize = font.size * uiZoom()
      safeFit(fitRef.current)
      document.fonts
        ?.load(`${Math.round(font.size)}px ${font.stack}`)
        .then(() => safeFit(fitRef.current))
        .catch(() => undefined)
    }
  }, [uiSize, font.stack, font.size])

  // A new token is the trigger. Token 0 is "no request", so a terminal that only opens does not
  // take the focus from the tab that opened it.
  const lastToken = useRef<number | undefined>(undefined)
  useEffect(() => {
    const changed = lastToken.current !== focusToken
    lastToken.current = focusToken
    if (changed && focusToken) {
      terminalRef.current?.focus()
    }
  }, [focusToken])

  // Mod+F is a command; it finds in the terminal that has the focus.
  useAppEvent('terminal-find', () => {
    if (container.current?.contains(document.activeElement)) {
      setFindOpen(true)
    }
  })

  const closeFind = () => {
    setFindOpen(false)
    terminalRef.current?.focus()
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      {status !== 'open' && (
        <div className="flex h-7 shrink-0 items-center gap-3 border-b border-line px-3">
          <p className="flex-1 truncate text-xs text-muted" role="status">
            {STATUS_TEXT[status]}
            {error !== null && ` ${error}`}
          </p>
          {(status === 'ended' || status === 'error') && (
            <Button variant="link" onClick={() => setAttempt((value) => value + 1)}>
              Start a new shell
            </Button>
          )}
        </div>
      )}
      <div className="hm-terminal relative flex min-h-0 flex-1 flex-col">
        {/* The fit addon reads the height of xterm's parent, padding included, so the padding is
            on a wrapper. With it on the parent, the last row fell under the bottom edge. */}
        <section
          aria-label="Terminal screen"
          className="flex min-h-0 flex-1 flex-col overflow-hidden py-2 pl-2.5"
        >
          <div ref={container} className="min-h-0 flex-1 overflow-hidden" />
        </section>
        {findOpen && search !== null && <TerminalFind search={search} onClose={closeFind} />}
        {scrolledUp && (
          <Button
            icon={ArrowDown}
            size="xs"
            onClick={() => terminalRef.current?.scrollToBottom()}
            className="absolute right-4 bottom-3 z-10 rounded-full! shadow-pop"
          >
            Jump to latest
          </Button>
        )}
      </div>
    </div>
  )
}
