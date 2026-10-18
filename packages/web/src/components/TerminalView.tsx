import type { TaskState } from '@natamux/core'
import '@xterm/xterm/css/xterm.css'
import { ArrowDown, SquareTerminal } from 'lucide-react'
import {
  type DragEvent,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  useSyncExternalStore,
} from 'react'
import { useAppEvent } from '../lib/app-events'
import { dropText, pathDragType } from '../lib/explorer-tree'
import { watchVisible } from '../lib/gpu-renderer'
import type { TerminalConnectQuery } from '../lib/terminal'
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
import { StatusGlyph } from './StatusGlyph'
import { TerminalFind } from './TerminalFind'
import { Button } from './ui/Button'
import { Skeleton } from './ui/Skeleton'
import { Spinner } from './ui/Spinner'

type TerminalViewProps = {
  connect: (query: TerminalConnectQuery) => Promise<WebSocket>
  // A new number takes the keyboard focus into the terminal, for "Answer in terminal".
  focusToken?: number
  // `${taskId}/${terminal}`: the terminal stays alive in the cache while no view shows it.
  // Without it, the terminal belongs to this view and ends with it.
  cacheKey?: string
  // The state of the shell's task. A shell of a done or cancelled task ends with its glyph.
  taskState?: TaskState
  // False: the terminal does not take the keyboard when it is shown.
  focusOnOpen?: boolean
  // The shell comes back after a restart: the line under the spinner, such as "Restoring Codex…".
  restoring?: string
  // Told once the shell ended: its exit code, or null when the daemon closed it.
  onExit?: (code: number | null) => void
  // A file or folder dragged from the files explorer of this task types its path here.
  dropTaskId?: string
}

// The drag data of a row of the files explorer, or null for anything else.
function draggedPath(raw: string): { path: string; kind: 'file' | 'folder' } | null {
  try {
    const { path, kind } = JSON.parse(raw)
    const known = typeof path === 'string' && (kind === 'file' || kind === 'folder')
    return known ? { path, kind } : null
  } catch {
    return null
  }
}

// The spinner of a shell that came back fades out in this long (duration-150 below).
export const RESTORE_FADE_MS = 150

const STATUS_TEXT: Record<TerminalStatus, string> = {
  connecting: 'Connecting.',
  open: 'Connected.',
  // The socket dropped while the shell runs. The screen stays; the missed output follows.
  reconnecting: 'Reconnecting.',
  ended: 'The shell ended.',
  error: 'The terminal could not start.',
}

const CONNECTING: TerminalSessionState = {
  status: 'connecting',
  error: null,
  scrolledUp: false,
  catchingUp: false,
  exitCode: null,
}
const noSubscribe = () => () => undefined

// The finished banner says once how the task ended. The exit code of a shell it ended says
// nothing useful.
function endedText(taskState: TaskState | undefined, code: number | null): string {
  const finished = taskState === 'done' || taskState === 'cancelled'
  return finished || code === null || code === 0
    ? STATUS_TEXT.ended
    : `${STATUS_TEXT.ended} Exit code ${code}.`
}

// A shell in the task's workspace. The default export is what ChangesView loads on demand.
export default function TerminalView({
  connect,
  focusToken,
  cacheKey,
  taskState,
  focusOnOpen = true,
  restoring,
  onExit,
  dropTaskId,
}: TerminalViewProps) {
  const container = useRef<HTMLDivElement>(null)
  const connectRef = useRef(connect)
  connectRef.current = connect
  const focusOnOpenRef = useRef(focusOnOpen)
  focusOnOpenRef.current = focusOnOpen
  const [attempt, setAttempt] = useState(0)
  const [session, setSession] = useState<TerminalSession | null>(null)
  // Set before paint, so effects and handlers of this commit already see the session.
  const sessionRef = useRef<TerminalSession | null>(null)
  const [findOpen, setFindOpen] = useState(false)
  const [dropping, setDropping] = useState(false)
  const theme = useResolvedTheme()
  const uiSize = useUiSize()
  const font = useTerminalFont()
  const subscribe = session?.subscribe ?? noSubscribe
  const snapshot = () => session?.state() ?? CONNECTING
  const { status, error, scrolledUp, catchingUp, exitCode } = useSyncExternalStore(
    subscribe,
    snapshot,
  )
  // The spinner covers the shell's prompt and the typed launch line until the agent runs. It stays
  // drawn while it fades out.
  const restoringNow = restoring !== undefined && status !== 'error' && status !== 'ended'
  const [restoreLine, setRestoreLine] = useState(restoring)
  useEffect(() => {
    if (restoringNow) {
      setRestoreLine(restoring)
      return
    }
    const timer = setTimeout(() => setRestoreLine(undefined), RESTORE_FADE_MS)
    return () => clearTimeout(timer)
  }, [restoringNow, restoring])

  const onExitRef = useRef(onExit)
  onExitRef.current = onExit
  useEffect(() => {
    if (status === 'ended') {
      onExitRef.current?.(exitCode)
    }
  }, [status, exitCode])

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

  // Shown after the render that has the session, still before the browser paints.
  useLayoutEffect(() => {
    const element = container.current
    if (!element || session === null) {
      return
    }
    session.show(element, focusOnOpenRef.current)
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

  // Only a drag of this task's explorer is taken. Its type names the task, and the data can only
  // be read on the drop. Any other drag keeps the browser's "not allowed".
  const dragType = dropTaskId === undefined ? null : pathDragType(dropTaskId)
  const takesDrag = (event: DragEvent) =>
    dragType !== null && status === 'open' && event.dataTransfer.types.includes(dragType)
  const dropEvents = {
    onDragOver: (event: DragEvent<HTMLDivElement>) => {
      if (takesDrag(event)) {
        event.preventDefault()
        event.dataTransfer.dropEffect = 'copy'
        setDropping(true)
      }
    },
    onDragLeave: (event: DragEvent<HTMLDivElement>) => {
      const next = event.relatedTarget
      if (!(next instanceof Node) || !event.currentTarget.contains(next)) {
        setDropping(false)
      }
    },
    onDrop: (event: DragEvent<HTMLDivElement>) => {
      setDropping(false)
      if (!takesDrag(event) || dragType === null) {
        return
      }
      event.preventDefault()
      const dragged = draggedPath(event.dataTransfer.getData(dragType))
      const terminal = sessionRef.current?.terminal
      if (dragged !== null && terminal !== undefined) {
        // A paste, so an agent in bracketed paste mode gets the path as one piece.
        terminal.paste(dropText(dragged.path, dragged.kind))
        terminal.focus()
      }
    },
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div
        {...dropEvents}
        className={`hm-terminal relative flex min-h-0 flex-1 flex-col ${
          dropping ? 'outline outline-2 -outline-offset-2 outline-focus' : ''
        }`}
      >
        {/* The fit addon reads the height of xterm's parent, padding included, so the padding is
            on a wrapper. With it on the parent, the last row fell under the bottom edge.
            xterm keeps room for its 8px scrollbar on the right, so 2px more there makes the same
            10px as on the left, with the scrollbar inside it. */}
        <section
          aria-label="Terminal screen"
          className={`flex min-h-0 flex-1 flex-col overflow-hidden py-2 pr-0.5 pl-2.5 ${
            status === 'ended' ? 'opacity-60' : ''
          }`}
        >
          {/* The session's own element, which moves between views, goes in here. */}
          <div ref={container} className="flex min-h-0 flex-1 flex-col overflow-hidden" />
        </section>
        {/* The screen is hidden while a catch-up is written, so no history scrolls by. The same
            skeleton as while the terminal's code loads. */}
        {catchingUp && (
          <div
            role="status"
            aria-label="Loading the terminal"
            className="pointer-events-none absolute inset-0"
          >
            <Skeleton className="m-3" width="30%" />
          </div>
        )}
        {restoreLine !== undefined && (
          <div
            role="status"
            className={`pointer-events-none absolute inset-0 flex flex-col items-center justify-center gap-2 bg-terminal text-xs text-muted transition-opacity duration-150 ${
              restoringNow ? 'opacity-100' : 'opacity-0'
            }`}
          >
            <Spinner size={16} />
            <p>{restoreLine}</p>
          </div>
        )}
        {findOpen && session !== null && (
          <TerminalFind search={session.search} onClose={closeFind} />
        )}
        {/* Over the screen, not above it: the screen keeps its box when the socket opens, so a new
            shell gets one size. */}
        {status !== 'open' && status !== 'ended' && (
          <div className="absolute inset-x-0 top-0 z-10 flex h-7 items-center gap-3 border-b border-line bg-surface px-3">
            {/* Shown late, so a quick connect does not flash it. */}
            {status !== 'error' && (
              <span className="hm-late-in flex shrink-0 text-muted">
                <Spinner size={12} />
              </span>
            )}
            <p className="flex-1 truncate text-xs text-muted" role="status">
              {STATUS_TEXT[status]}
              {error !== null && ` ${error}`}
            </p>
            {status === 'error' && (
              <Button variant="link" onClick={startNewShell}>
                Start a new shell
              </Button>
            )}
          </div>
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
      {status === 'ended' && (
        <div className="flex h-8 shrink-0 items-center gap-2 border-t border-line bg-surface px-3">
          {taskState === 'done' || taskState === 'cancelled' ? (
            <StatusGlyph state={taskState} />
          ) : (
            <SquareTerminal size={14} aria-hidden className="shrink-0 text-muted" />
          )}
          <p className="flex-1 truncate text-xs text-ink-2" role="status">
            {endedText(taskState, exitCode)}
          </p>
          <Button
            variant="link"
            data-tip="Starts a plain shell in the task's workspace. It does not start the agent."
            onClick={startNewShell}
          >
            Open a new shell
          </Button>
        </div>
      )}
    </div>
  )
}
