import type { TerminalSize } from '@helmsman/core'
import { FitAddon } from '@xterm/addon-fit'
import { Terminal } from '@xterm/xterm'
import '@xterm/xterm/css/xterm.css'
import { useEffect, useRef, useState } from 'react'
import { parseTerminalMessage } from '../lib/terminal'
import {
  MIN_CONTRAST_RATIO,
  TERMINAL_FONT_SIZE,
  TERMINAL_LINE_HEIGHT,
  terminalTheme,
} from '../lib/terminal-theme'
import { uiZoom, useResolvedTheme } from '../lib/theme'
import { useUiSize } from '../lib/ui-size'
import { Button } from './ui/Button'

type Status = 'connecting' | 'open' | 'ended' | 'error'

// The same stack as --font-mono in styles.css. xterm needs the names, not a CSS variable.
const MONO_FONT =
  '"Geist Mono Variable", ui-monospace, "SF Mono", "Cascadia Mono", Menlo, Consolas, monospace'

type TerminalViewProps = {
  connect: (size: TerminalSize) => Promise<WebSocket>
}

const STATUS_TEXT: Record<Status, string> = {
  connecting: 'Connecting.',
  open: 'Connected.',
  ended: 'The shell ended.',
  error: 'The terminal could not start.',
}

// A shell in the task's workspace. The default export is what ChangesView loads on demand.
export default function TerminalView({ connect }: TerminalViewProps) {
  const container = useRef<HTMLElement>(null)
  const connectRef = useRef(connect)
  connectRef.current = connect
  const [status, setStatus] = useState<Status>('connecting')
  const [error, setError] = useState<string | null>(null)
  const [attempt, setAttempt] = useState(0)
  const theme = useResolvedTheme()
  const themeRef = useRef(theme)
  themeRef.current = theme
  const terminalRef = useRef<Terminal | null>(null)
  const fitRef = useRef<FitAddon | null>(null)
  const uiSize = useUiSize()

  // biome-ignore lint/correctness/useExhaustiveDependencies: attempt starts a new shell on purpose.
  useEffect(() => {
    const element = container.current
    if (!element) {
      return
    }
    const terminal = new Terminal({
      cursorBlink: true,
      fontFamily: MONO_FONT,
      fontSize: TERMINAL_FONT_SIZE * uiZoom(),
      lineHeight: TERMINAL_LINE_HEIGHT,
      minimumContrastRatio: MIN_CONTRAST_RATIO,
      scrollback: 5000,
      theme: terminalTheme(themeRef.current),
    })
    terminalRef.current = terminal
    const fit = new FitAddon()
    terminal.loadAddon(fit)
    fitRef.current = fit
    // F6 moves between the parts of the window. The shell never sees it.
    terminal.attachCustomKeyEventHandler((event) => event.key !== 'F6')
    terminal.open(element)
    fit.fit()

    let socket: WebSocket | null = null
    let disposed = false
    const send = (message: object) => {
      if (socket?.readyState === WebSocket.OPEN) {
        socket.send(JSON.stringify(message))
      }
    }

    setStatus('connecting')
    setError(null)
    connectRef
      .current({ cols: terminal.cols, rows: terminal.rows })
      .then((opened) => {
        if (disposed) {
          opened.close()
          return
        }
        socket = opened
        opened.onopen = () => setStatus('open')
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
          if (!disposed) {
            setStatus((current) => (current === 'error' ? current : 'ended'))
          }
        }
      })
      .catch((reason: unknown) => {
        if (!disposed) {
          setError(reason instanceof Error ? reason.message : String(reason))
          setStatus('error')
        }
      })

    const input = terminal.onData((data) => send({ type: 'input', data }))
    const resized = terminal.onResize(({ cols, rows }) => send({ type: 'resize', cols, rows }))
    const observer = new ResizeObserver(() => fit.fit())
    observer.observe(element)
    terminal.focus()

    // The web font may still be loading when the terminal measures its cells. Setting the font
    // again makes it measure with the loaded one.
    document.fonts
      ?.load('13px "Geist Mono Variable"')
      .then(() => {
        if (!disposed) {
          terminal.options.fontFamily = 'monospace'
          terminal.options.fontFamily = MONO_FONT
          fit.fit()
        }
      })
      .catch(() => undefined)

    return () => {
      disposed = true
      observer.disconnect()
      input.dispose()
      resized.dispose()
      socket?.close()
      terminal.dispose()
      terminalRef.current = null
      fitRef.current = null
    }
  }, [attempt])

  useEffect(() => {
    if (terminalRef.current) {
      terminalRef.current.options.theme = terminalTheme(theme)
    }
  }, [theme])

  // The UI size setting changes the font, and then the number of columns.
  // biome-ignore lint/correctness/useExhaustiveDependencies: uiSize is the trigger; uiZoom() reads the page.
  useEffect(() => {
    if (terminalRef.current) {
      terminalRef.current.options.fontSize = TERMINAL_FONT_SIZE * uiZoom()
      fitRef.current?.fit()
    }
  }, [uiSize])

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      {status !== 'open' && (
        <div className="flex items-center gap-3 px-3 pt-2">
          <p className="flex-1 text-xs text-muted" role="status">
            {STATUS_TEXT[status]}
            {error !== null && ` ${error}`}
          </p>
          {(status === 'ended' || status === 'error') && (
            <Button size="xs" onClick={() => setAttempt((value) => value + 1)}>
              Start a new shell
            </Button>
          )}
        </div>
      )}
      <section
        ref={container}
        aria-label="Terminal screen"
        className="min-h-0 flex-1 overflow-hidden px-3 py-2"
      />
    </div>
  )
}
