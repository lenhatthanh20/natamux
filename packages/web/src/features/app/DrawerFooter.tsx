import { Download, FolderGit2, Keyboard } from 'lucide-react'
import { IconButton } from '../../components/ui/IconButton'
import { AccountMenu } from './AccountMenu'
import { LIVE_TEXT } from './StatusBar'
import type { LiveStatus } from './useLiveTasks'

type DrawerFooterProps = {
  liveStatus: LiveStatus
  running: number
  needsYou: number
  onRepos: () => void
  onImport: () => void
  onShortcuts: () => void
  onSettings: () => void
  onSignOut: () => void
}

// Under 768 there is no status bar: its items sit at the bottom of the sidebar drawer.
export function DrawerFooter({
  liveStatus,
  running,
  needsYou,
  onRepos,
  onImport,
  onShortcuts,
  onSettings,
  onSignOut,
}: DrawerFooterProps) {
  return (
    <div className="flex shrink-0 flex-col gap-1 border-t border-line px-3 py-2">
      <p className="flex items-center gap-2 text-xs text-muted">
        <span
          aria-hidden="true"
          className={`size-1.5 rounded-full ${liveStatus === 'live' ? 'bg-add' : 'bg-signal'}`}
        />
        <span>{LIVE_TEXT[liveStatus]}</span>
        <span>{running} running</span>
        {needsYou > 0 && (
          <span className="text-signal">
            {needsYou} {needsYou === 1 ? 'needs' : 'need'} you
          </span>
        )}
      </p>
      <div className="flex items-center gap-1">
        <IconButton label="Projects" icon={FolderGit2} onClick={onRepos} />
        <IconButton label="Import issues" icon={Download} onClick={onImport} />
        <IconButton label="Keyboard shortcuts" icon={Keyboard} onClick={onShortcuts} />
        <span className="flex-1" />
        <AccountMenu onSignOut={onSignOut} onShortcuts={onShortcuts} onSettings={onSettings} />
      </div>
    </div>
  )
}
