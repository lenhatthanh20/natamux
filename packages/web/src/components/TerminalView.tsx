import type { TerminalSize } from '@helmsman/core'
import { FitAddon } from '@xterm/addon-fit'
import { type ITheme, Terminal } from '@xterm/xterm'
import '@xterm/xterm/css/xterm.css'
import { useEffect, useRef, useState } from 'react'
import { parseTerminalMessage } from '../lib/terminal'

type Status = 'connecting' | 'open' | 'ended' | 'error'

// The same stack as --font-mono in styles.css. xterm needs the names, not a CSS variable.
const MONO_FONT =
  '"Geist Mono Variable", ui-monospace, "SF Mono", "Cascadia Mono", Menlo, Consolas, monospace'

type TerminalViewProps = {
  connect: (size: TerminalSize) => Promise<WebSocket>
}

// Colors come from the dashboard's own tokens, so the terminal follows light and dark.
function themeFromPage(): ITheme {
  const style = getComputedStyle(document.documentElement)
  const token = (name: string) => style.getPropertyValue(name).trim()
  return {
    background: token('--terminal'),
    foreground: token('--ink'),
    cursor: token('--accent'),
    selectionBackground: `${token('--accent')}55`,
  }
}

const STATUS_TEXT: Record<Status, string> = {
  connecting: 'Connecting.',
  open: 'The shell keeps running when you leave this tab.',
  ended: 'The shell ended.',
  error: 'The terminal could not start.',
}

// A shell in the task's workspace. The default export is what ChangesView loads on demand.
export default function TerminalView({ connect }: TerminalViewProps) {
  const container = useRef<HTMLDivElement>(null)
  const connectRef = useRef(connect)
  connectRef.current = connect
  const [status, setStatus] = useState<Status>('connecting')
  const [error, setError] = useState<string | null>(null)
  const [attempt, setAttempt] = useState(0)

  // biome-ignore lint/correctness/useExhaustiveDependencies: attempt starts a new shell on purpose.
  useEffect(() => {
    const element = container.current
    if (!element) {
      return
    }
    const terminal = new Terminal({
      cursorBlink: true,
      fontFamily: MONO_FONT,
      fontSize: 13,
      scrollback: 5000,
      theme: themeFromPage(),
    })
    const fit = new FitAddon()
    terminal.loadAddon(fit)
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
    }
  }, [attempt])

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-2">
      <div className="flex items-center gap-3">
        <p className="flex-1 text-xs text-muted" role="status">
          {STATUS_TEXT[status]}
          {error !== null && ` ${error}`}
        </p>
        {(status === 'ended' || status === 'error') && (
          <button
            type="button"
            onClick={() => setAttempt((value) => value + 1)}
            className="rounded-md border border-line px-3 py-1.5 font-medium hover:border-accent"
          >
            Start a new shell
          </button>
        )}
      </div>
      <div
        ref={container}
        title="Terminal"
        className="min-h-[16rem] flex-1 overflow-hidden rounded-lg border border-line bg-terminal p-3"
      />
    </div>
  )
}
