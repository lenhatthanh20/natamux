import type { TerminalSize } from '@helmsman/core'
import { X } from 'lucide-react'
import { lazy, type ReactNode, Suspense } from 'react'
import { IconButton } from './ui/IconButton'

// xterm.js is loaded the first time a terminal is shown.
const TerminalView = lazy(() => import('./TerminalView'))

type TerminalPaneProps = {
  // The pane's name for screen readers, for example "Shell 2".
  label: string
  header: ReactNode
  connect: (size: TerminalSize) => Promise<WebSocket>
  onClose?: () => void
  // Kept mounted, so the shell keeps running, but not drawn.
  hidden?: boolean
}

// One terminal with a thin title bar, sized by the grid around it.
export function TerminalPane({ label, header, connect, onClose, hidden }: TerminalPaneProps) {
  return (
    <section
      aria-label={label}
      hidden={hidden}
      className="flex min-h-0 min-w-0 flex-col overflow-hidden rounded-lg border border-line bg-terminal"
    >
      <header className="flex items-center gap-2 border-b border-line px-3 py-1.5 text-xs">
        <div className="flex min-w-0 flex-1 items-center gap-2">{header}</div>
        {onClose !== undefined && (
          <IconButton label={`Close ${label}`} icon={X} size="xs" onClick={onClose} />
        )}
      </header>
      <Suspense fallback={<p className="p-3 text-muted">Loading the terminal.</p>}>
        <TerminalView connect={connect} />
      </Suspense>
    </section>
  )
}
