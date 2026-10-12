import type { TerminalSize } from '@natamux/core'
import '@xterm/xterm/css/xterm.css'
import { ArrowDown } from 'lucide-react'
import { useEffect, useLayoutEffect, useRef, useState, useSyncExternalStore } from 'react'
import { useAppEvent } from '../lib/app-events'
import { watchVisible } from '../lib/gpu-renderer'
import { acquireTerminal, discardTerminal } from '../lib/terminal-cache'
import { useTerminalFont } from '../lib/terminal-font'
import {
  createTerminalSession,
  type TerminalSession,
  type TerminalSessionState,
  type TerminalStatus,
} from '../lib/terminal-session'
import { useResolvedTheme } from '../lib/theme'
import { useUiSize } from '../lib/ui-size'
import { TerminalFind } from './TerminalFind'
import { Button } from './ui/Button'

type TerminalViewProps = {
  connect: (size: TerminalSize) => Promise<WebSocket>
  // A new number takes the keyboard focus into the terminal, for "Answer in terminal".
  focusToken?: number
  // `${taskId}/${terminal}`: the terminal stays alive in the cache while no view shows it.
  // Without it, the terminal belongs to this view and ends with it.
  cacheKey?: string
}

const STATUS_TEXT: Record<TerminalStatus, string> = {
  connecting: 'Connecting.',
  open: 'Connected.',
  ended: 'The shell ended.',
  error: 'The terminal could not start.',
}

const CONNECTING: TerminalSessionState = { status: 'connecting', error: null, scrolledUp: false }
const noSubscribe = () => () => undefined

// A shell in the task's workspace. The default export is what ChangesView loads on demand.
export default function TerminalView({ connect, focusToken, cacheKey }: TerminalViewProps) {
  const container = useRef<HTMLDivElement>(null)
  const connectRef = useRef(connect)
  connectRef.current = connect
  const [attempt, setAttempt] = useState(0)
  const [session, setSession] = useState<TerminalSession | null>(null)
  // Set before paint, so effects and handlers of this commit already see the session.
  const sessionRef = useRef<TerminalSession | null>(null)
  const [findOpen, setFindOpen] = useState(false)
  const theme = useResolvedTheme()
  const uiSize = useUiSize()
  const font = useTerminalFont()
  const subscribe = session?.subscribe ?? noSubscribe
  const snapshot = () => session?.state() ?? CONNECTING
  const { status, error, scrolledUp } = useSyncExternalStore(subscribe, snapshot)

  // Layout effects, so a cached terminal is back in place before the browser paints.
  // biome-ignore lint/correctness/useExhaustiveDependencies: attempt starts a new shell on purpose.
  useLayoutEffect(() => {
    const create = () =>
      createTerminalSession({ connect: connectRef.current, onEnd: discardTerminal })
    const lease = acquireTerminal(cacheKey, create)
    sessionRef.current = lease.session
    setSession(lease.session)
    return () => {
      lease.release()
      sessionRef.current = null
      setSession(null)
      setFindOpen(false)
    }
  }, [attempt, cacheKey])

  // Shown only after the render that has the session's status. Before it, the status strip of
  // "connecting" takes room, so a cached open terminal would fit a smaller box and send two
  // resizes: one now, and one back when the strip goes.
  useLayoutEffect(() => {
    const element = container.current
    if (!element || session === null) {
      return
    }
    session.show(element)
    return watchVisible(element, session.setOnScreen)
  }, [session])

  // Theme, font and UI size change the colors and the cells. A hidden terminal takes them when
  // it is shown again.
  // biome-ignore lint/correctness/useExhaustiveDependencies: the settings are triggers.
  useEffect(() => {
    session?.applySettings()
  }, [session, theme, font.stack, font.size, uiSize])

  // A new token is the trigger. Token 0 is "no request", so a terminal that only opens does not
  // take the focus from the tab that opened it.
  const lastToken = useRef<number | undefined>(undefined)
  useEffect(() => {
    const changed = lastToken.current !== focusToken
    lastToken.current = focusToken
    if (changed && focusToken) {
      sessionRef.current?.terminal.focus()
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
    sessionRef.current?.terminal.focus()
  }

  // The old terminal is dropped from the cache, so the next run makes a fresh one.
  const startNewShell = () => {
    if (sessionRef.current !== null) {
      discardTerminal(sessionRef.current)
    }
    setAttempt((value) => value + 1)
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
            <Button variant="link" onClick={startNewShell}>
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
          {/* The session's own element, which moves between views, goes in here. */}
          <div ref={container} className="flex min-h-0 flex-1 flex-col overflow-hidden" />
        </section>
        {findOpen && session !== null && (
          <TerminalFind search={session.search} onClose={closeFind} />
        )}
        {scrolledUp && (
          <Button
            icon={ArrowDown}
            size="xs"
            onClick={() => sessionRef.current?.terminal.scrollToBottom()}
            className="absolute right-4 bottom-3 z-10 rounded-full! shadow-pop"
          >
            Jump to latest
          </Button>
        )}
      </div>
    </div>
  )
}
