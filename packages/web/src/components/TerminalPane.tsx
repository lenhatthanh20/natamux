import { ExternalLink, Maximize2, Minimize2, X } from 'lucide-react'
import { type CSSProperties, lazy, type ReactNode, useRef } from 'react'
import { useAppEvent } from '../lib/app-events'
import { shortcutOf } from '../lib/commands'
import type { TerminalConnectQuery } from '../lib/terminal'
import { AgentBadge } from './AgentBadge'
import { IconButton } from './ui/IconButton'
import { LazyView } from './ui/LazyView'
import { Skeleton } from './ui/Skeleton'

// xterm.js is loaded the first time a terminal is shown, or earlier when the page is idle.
export const loadTerminalView = () => import('./TerminalView')
const TerminalView = lazy(loadTerminalView)

type TerminalPaneProps = {
  // The pane's name for screen readers, for example "Shell 2".
  label: string
  // The icon before the title: a shell icon, or the state glyph of an agent.
  icon: ReactNode
  title: ReactNode
  // Set only when an agent runs in this pane: its id, or null when the task has none.
  agent?: string | null
  // The line after the title: the state and what the agent does now.
  stateText?: string
  // The agent waits for the user: the pane shows a bar and a tint.
  needsInput?: boolean
  // Another agent of the task needs the user, such as "Shell 2 needs your input".
  hint?: string
  connect: (query: TerminalConnectQuery) => Promise<WebSocket>
  focusToken?: number
  // Keeps the terminal alive while the pane is gone, see lib/terminal-cache.ts.
  cacheKey?: string
  maximized: boolean
  onToggleMaximize: () => void
  // The agents grid can open the workspace of a pane.
  openWorkspace?: { label: string; run: () => void }
  onClose?: () => void
  closeLabel?: string
  // The tooltip of the close button, when it says more than the label.
  closeTip?: string
  // Kept mounted, so the shell keeps running, but not drawn.
  hidden?: boolean
  style?: CSSProperties
}

// One terminal with a 32px title bar, sized by the grid around it.
export function TerminalPane({
  label,
  icon,
  title,
  agent,
  stateText,
  needsInput = false,
  hint,
  connect,
  focusToken,
  cacheKey,
  maximized,
  onToggleMaximize,
  openWorkspace,
  onClose,
  closeLabel,
  closeTip,
  hidden,
  style,
}: TerminalPaneProps) {
  const pane = useRef<HTMLElement>(null)
  // Mod+Shift+Enter is a command; it maximizes the pane that has the focus.
  useAppEvent('pane-maximize', () => {
    if (pane.current?.contains(document.activeElement)) {
      onToggleMaximize()
    }
  })
  return (
    <section
      ref={pane}
      aria-label={label}
      hidden={hidden}
      style={style}
      className="group/pane relative flex min-h-0 min-w-0 flex-col overflow-hidden rounded-lg border border-line-strong bg-terminal ring-contrast focus-within:outline focus-within:outline-1 focus-within:-outline-offset-1 focus-within:outline-focus"
    >
      {needsInput && (
        <span aria-hidden="true" className="absolute inset-x-0 top-0 z-10 h-0.5 bg-signal" />
      )}
      <header
        className={`@container flex h-8 shrink-0 items-center gap-2 border-b border-line pr-1 pl-2.5 text-xs ${
          needsInput ? 'bg-signal-soft' : 'bg-surface'
        }`}
      >
        <span className="flex shrink-0 items-center text-muted">{icon}</span>
        <span className="flex min-w-0 items-center gap-1.5 font-medium text-muted group-focus-within/pane:text-ink">
          {title}
        </span>
        {agent !== undefined && <AgentBadge agent={agent} />}
        {stateText !== undefined && (
          <span
            data-tip-overflow
            className={`min-w-0 flex-1 truncate ${needsInput ? 'text-signal' : 'text-muted'}`}
          >
            {stateText}
          </span>
        )}
        {stateText === undefined && <span className="flex-1" />}
        {hint !== undefined && (
          <span data-tip-overflow className="min-w-0 truncate text-signal">
            {hint}
          </span>
        )}
        <IconButton
          label={maximized ? `Restore ${label}` : `Maximize ${label}`}
          icon={maximized ? Minimize2 : Maximize2}
          size="xs"
          shortcut={shortcutOf('pane.maximize')}
          aria-pressed={maximized}
          onClick={onToggleMaximize}
        />
        {openWorkspace !== undefined && (
          <IconButton
            label={openWorkspace.label}
            icon={ExternalLink}
            size="xs"
            onClick={openWorkspace.run}
          />
        )}
        {onClose !== undefined && (
          <IconButton
            label={closeLabel ?? `Close ${label}`}
            tip={closeTip}
            icon={X}
            size="xs"
            onClick={onClose}
          />
        )}
      </header>
      <LazyView label="terminal" fallback={<Skeleton className="m-3" width="30%" />}>
        <TerminalView connect={connect} focusToken={focusToken} cacheKey={cacheKey} />
      </LazyView>
    </section>
  )
}
