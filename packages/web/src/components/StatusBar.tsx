import {
  Bell,
  BellOff,
  Download,
  Ellipsis,
  FolderGit2,
  Keyboard,
  type LucideIcon,
} from 'lucide-react'
import { useLayoutEffect, useRef, useState } from 'react'
import type { LiveStatus } from '../hooks/useLiveTasks'
import type { NotifyPermission } from '../hooks/useTaskNotifications'
import {
  condense,
  forcedByWidth,
  maxCondenseLevel,
  mergeCondensed,
  type StatusItem,
} from '../lib/status-bar'
import { AccountMenu } from './AccountMenu'
import { IconButton } from './ui/IconButton'
import { Menu, type MenuItem } from './ui/Menu'
import { ICON } from './ui/styles'

export const LIVE_TEXT: Record<LiveStatus, string> = {
  live: 'Live',
  connecting: 'Connecting',
  reconnecting: 'Reconnecting',
}

const LIVE_TIP: Record<LiveStatus, string> = {
  live: 'Changes appear as they happen',
  connecting: 'Connecting to the daemon',
  reconnecting: 'Lost the connection. Trying to reconnect.',
}

type StatusBarProps = {
  liveStatus: LiveStatus
  running: number
  needsYou: number
  onFilterRunning: () => void
  onNeedsYou: () => void
  onRepos: () => void
  onImport: () => void
  notifications: { permission: NotifyPermission; askPermission: () => void }
  onShortcuts: () => void
  onSignOut: () => void
  // The window width, for the rules that do not depend on measuring. Null: no rule.
  width: number | null
}

type Entry = {
  id: string
  priority: number
  label: string
  // What the tooltip says when the label is hidden or the item needs a hint.
  tip: string
  icon?: LucideIcon
  dot?: 'live' | 'wait'
  tone?: 'signal'
  onSelect?: () => void
}

function BarItem({ entry, iconOnly }: { entry: Entry; iconOnly: boolean }) {
  const Icon = entry.icon
  const content = (
    <>
      {entry.dot !== undefined && (
        <span
          aria-hidden="true"
          className={`size-1.5 shrink-0 rounded-full ${entry.dot === 'live' ? 'bg-add' : 'bg-signal'}`}
        />
      )}
      {Icon !== undefined && <Icon size={ICON.sm} aria-hidden="true" className="shrink-0" />}
      {!iconOnly && <span>{entry.label}</span>}
    </>
  )
  const className = `flex h-6 shrink-0 items-center gap-1.5 rounded-md px-1.5 text-xs whitespace-nowrap ${
    entry.tone === 'signal' ? 'text-signal' : 'text-muted'
  }`
  if (entry.onSelect === undefined) {
    // A status, not a control.
    return (
      <span
        role="status"
        aria-label={iconOnly ? entry.label : undefined}
        data-tip={entry.tip}
        data-tip-side="top"
        className={className}
      >
        {content}
      </span>
    )
  }
  return (
    <button
      type="button"
      aria-label={iconOnly ? entry.label : undefined}
      data-tip={entry.tip}
      data-tip-side="top"
      onClick={entry.onSelect}
      className={`${className} hover:bg-hover ${entry.tone === 'signal' ? '' : 'hover:text-ink'} focus-visible:outline-2`}
    >
      {content}
    </button>
  )
}

// The bottom bar. It never wraps: when it is too narrow, the lowest priority items lose their
// labels first, then move into the "..." menu.
export function StatusBar({
  liveStatus,
  running,
  needsYou,
  onFilterRunning,
  onNeedsYou,
  onRepos,
  onImport,
  notifications,
  onShortcuts,
  onSignOut,
  width,
}: StatusBarProps) {
  const bar = useRef<HTMLElement>(null)
  const [level, setLevel] = useState(0)
  const [barWidth, setBarWidth] = useState(0)
  const lastKey = useRef('')

  const entries: Entry[] = [
    {
      id: 'live',
      priority: 1,
      label: LIVE_TEXT[liveStatus],
      tip: LIVE_TIP[liveStatus],
      dot: liveStatus === 'live' ? 'live' : 'wait',
    },
    {
      id: 'running',
      priority: 2,
      label: `${running} running`,
      tip: 'Show the running tasks',
      onSelect: onFilterRunning,
    },
    ...(needsYou > 0
      ? [
          {
            id: 'needs',
            priority: 3,
            label: `${needsYou} ${needsYou === 1 ? 'needs' : 'need'} you`,
            tip: 'Show the tasks that wait for you',
            tone: 'signal' as const,
            onSelect: onNeedsYou,
          },
        ]
      : []),
    {
      id: 'repos',
      priority: 6,
      label: 'Repos',
      tip: 'Repositories',
      icon: FolderGit2,
      onSelect: onRepos,
    },
    {
      id: 'import',
      priority: 7,
      label: 'Import issues',
      tip: 'Import GitHub issues as tasks',
      icon: Download,
      onSelect: onImport,
    },
    ...(notifications.permission === 'default'
      ? [
          {
            id: 'notifications',
            priority: 5,
            label: 'Notify me',
            tip: 'A system notification when a task starts to wait for you',
            icon: Bell,
            onSelect: notifications.askPermission,
          },
        ]
      : []),
    ...(notifications.permission === 'denied'
      ? [
          {
            id: 'notifications',
            priority: 5,
            label: 'Notifications are blocked',
            tip: "Notifications are blocked. Allow them in the browser's site settings.",
            icon: BellOff,
          },
        ]
      : []),
  ]
  const statusItems: StatusItem[] = entries.map((entry) => ({
    id: entry.id,
    priority: entry.priority,
    hasLabel: true,
  }))
  const maxLevel = maxCondenseLevel(statusItems)
  const { iconOnly, folded } = mergeCondensed(forcedByWidth(width), condense(statusItems, level))

  // Measure again when the bar or its text changes, then take things away until it fits.
  const key = `${barWidth}|${width}|${entries.map((entry) => `${entry.id}:${entry.label}`).join(',')}`
  useLayoutEffect(() => {
    const element = bar.current
    if (element === null) {
      return
    }
    if (key !== lastKey.current) {
      lastKey.current = key
      if (level !== 0) {
        setLevel(0)
        return
      }
    }
    if (element.scrollWidth > element.clientWidth && level < maxLevel) {
      setLevel(level + 1)
    }
  })

  useLayoutEffect(() => {
    const element = bar.current
    if (element === null || typeof ResizeObserver === 'undefined') {
      return
    }
    const observer = new ResizeObserver(() => setBarWidth(element.clientWidth))
    observer.observe(element)
    return () => observer.disconnect()
  }, [])

  const left = entries.filter((entry) => ['live', 'running', 'needs'].includes(entry.id))
  const right = entries.filter((entry) => !['live', 'running', 'needs'].includes(entry.id))
  const overflow: MenuItem[] = entries
    .filter((entry) => folded.has(entry.id))
    .map((entry) => ({
      label: entry.label,
      icon: entry.icon,
      disabled: entry.onSelect === undefined,
      onSelect: entry.onSelect ?? (() => undefined),
    }))

  return (
    <footer
      ref={bar}
      aria-label="Status"
      className="flex h-7 shrink-0 items-center gap-0.5 overflow-hidden border-t border-line bg-shell px-1.5"
    >
      {left
        .filter((entry) => !folded.has(entry.id))
        .map((entry) => (
          <BarItem key={entry.id} entry={entry} iconOnly={iconOnly.has(entry.id)} />
        ))}
      <span className="flex-1" />
      {right
        .filter((entry) => !folded.has(entry.id))
        .map((entry) => (
          <BarItem key={entry.id} entry={entry} iconOnly={iconOnly.has(entry.id)} />
        ))}
      {overflow.length > 0 && (
        <Menu label="More status items" items={overflow} icon={Ellipsis} tipSide="top" />
      )}
      <IconButton
        label="Keyboard shortcuts"
        icon={Keyboard}
        shortcut="?"
        size="xs"
        tipSide="top"
        onClick={onShortcuts}
      />
      <AccountMenu onSignOut={onSignOut} onShortcuts={onShortcuts} />
    </footer>
  )
}
