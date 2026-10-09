import type { TerminalSize } from '@helmsman/core'
import { ExternalLink, Maximize2, Minimize2, X } from 'lucide-react'
import { type CSSProperties, lazy, type ReactNode, Suspense } from 'react'
import { terminalShortcut } from '../lib/terminal-keys'
import { IconButton } from './ui/IconButton'
import { Skeleton } from './ui/Skeleton'

// xterm.js is loaded the first time a terminal is shown.
const TerminalView = lazy(() => import('./TerminalView'))

type TerminalPaneProps = {
  // The pane's name for screen readers, for example "Shell 2".
  label: string
  // The icon before the title: a shell icon, or the state glyph of an agent.
  icon: ReactNode
  title: ReactNode
  // "Agent", only when an agent runs in this pane.
  role?: string
  // The line after the title: the state and what the agent does now.
  stateText?: string
  // The agent waits for the user: the pane shows a bar and a tint.
  needsInput?: boolean
  connect: (size: TerminalSize) => Promise<WebSocket>
  focusToken?: number
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
  role,
  stateText,
  needsInput = false,
  connect,
  focusToken,
  maximized,
  onToggleMaximize,
  openWorkspace,
  onClose,
  closeLabel,
  closeTip,
  hidden,
  style,
}: TerminalPaneProps) {
  return (
    <section
      aria-label={label}
      hidden={hidden}
      style={style}
      onKeyDown={(event) => {
        // The terminal lets this key through to the page, see TerminalView.
        if (terminalShortcut(event.nativeEvent) === 'maximize') {
          event.preventDefault()
          onToggleMaximize()
        }
      }}
      className="group/pane relative flex min-h-0 min-w-0 flex-col overflow-hidden rounded-lg border border-line-strong bg-terminal ring-contrast focus-within:outline focus-within:outline-1 focus-within:-outline-offset-1 focus-within:outline-focus"
    >
      {needsInput && (
        <span aria-hidden="true" className="absolute inset-x-0 top-0 z-10 h-0.5 bg-signal" />
      )}
      <header
        className={`flex h-8 shrink-0 items-center gap-2 border-b border-line pr-1 pl-2.5 text-xs ${
          needsInput ? 'bg-signal-soft' : 'bg-surface'
        }`}
      >
        <span className="flex shrink-0 items-center text-muted">{icon}</span>
        <span className="flex min-w-0 items-center gap-1.5 font-medium text-muted group-focus-within/pane:text-ink">
          {title}
        </span>
        {role !== undefined && (
          <span className="shrink-0 rounded border border-line px-1.5 text-2xs font-medium text-ink-2">
            {role}
          </span>
        )}
        {stateText !== undefined && (
          <span
            data-tip-overflow
            className={`min-w-0 flex-1 truncate ${needsInput ? 'text-signal' : 'text-muted'}`}
          >
            {stateText}
          </span>
        )}
        {stateText === undefined && <span className="flex-1" />}
        <IconButton
          label={maximized ? `Restore ${label}` : `Maximize ${label}`}
          icon={maximized ? Minimize2 : Maximize2}
          size="xs"
          shortcut="Mod+Shift+Enter"
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
      <Suspense fallback={<Skeleton className="m-3" width="30%" />}>
        <TerminalView connect={connect} focusToken={focusToken} />
      </Suspense>
    </section>
  )
}
