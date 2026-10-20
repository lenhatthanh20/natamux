import {
  Bell,
  BellOff,
  Download,
  Ellipsis,
  FolderGit2,
  Keyboard,
  type LucideIcon,
} from 'lucide-react'
import { type ReactNode, type RefObject, useLayoutEffect, useRef, useState } from 'react'
import { IconButton } from '../../components/ui/IconButton'
import { Menu, type MenuItem } from '../../components/ui/Menu'
import { Spinner } from '../../components/ui/Spinner'
import { ICON } from '../../components/ui/styles'
import { useNow } from '../../hooks/useNow'
import type { Job } from '../../lib/api'
import { shortcutOf } from '../../lib/commands'
import { AccountMenu } from './AccountMenu'
import { jobRows, jobsLabel } from './background-jobs'
import {
  condense,
  forcedByWidth,
  maxCondenseLevel,
  mergeCondensed,
  type StatusItem,
} from './status-bar'
import type { LiveStatus } from './useLiveTasks'
import type { NotifyPermission } from './useTaskNotifications'

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
  // The daemon's background jobs that run now, oldest first.
  jobs: readonly Job[]
  onOpenTask: (taskId: string) => void
  onFilterRunning: () => void
  onNeedsYou: () => void
  onRepos: () => void
  onImport: () => void
  notifications: { permission: NotifyPermission; askPermission: () => void }
  onShortcuts: () => void
  onSettings: () => void
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
  // A spinner in place of the icon.
  busy?: boolean
  tone?: 'signal'
  onSelect?: () => void
  // A list that opens from the item, by the time it is now.
  menu?: (now: Date) => MenuItem[]
}

const LEFT = ['live', 'running', 'needs', 'jobs']

// A status item that opens a list. Its own part, so the clock re-renders only it.
function BarMenu({
  entry,
  menu,
  content,
  className,
  iconOnly,
}: {
  entry: Entry
  menu: (now: Date) => MenuItem[]
  content: ReactNode
  className: string
  iconOnly: boolean
}) {
  const now = useNow()
  return (
    <Menu
      label={entry.label}
      items={menu(now)}
      trigger={(props) => (
        <button
          type="button"
          {...props}
          aria-label={iconOnly ? entry.label : undefined}
          data-item={entry.id}
          data-tip={entry.tip}
          data-tip-side="top"
          className={`${className} hover:bg-hover hover:text-ink focus-visible:outline-2`}
        >
          {content}
        </button>
      )}
    />
  )
}

// An item that goes away while it, or its open menu, has focus, such as the jobs item with its last
// job, would leave focus on the page. Focus moves to the bar's first other control, as F6 does.
// The cleanup runs before React removes the item, so the focus is still there to see.
function useFocusOnRemove(bar: RefObject<HTMLElement | null>, id: string) {
  useLayoutEffect(() => {
    return () => {
      const own = bar.current?.querySelector(`[data-item="${id}"]`)
      const active = document.activeElement
      if (!own || !active) {
        return
      }
      const inOpenMenu =
        own.getAttribute('aria-expanded') === 'true' && active.closest('[role="menu"]') !== null
      if (own.contains(active) || inOpenMenu) {
        const others = [...(bar.current?.querySelectorAll<HTMLElement>('button') ?? [])]
        others.find((button) => !own.contains(button))?.focus()
      }
    }
  }, [bar, id])
}

function BarItem({
  entry,
  iconOnly,
  bar,
}: {
  entry: Entry
  iconOnly: boolean
  bar: RefObject<HTMLElement | null>
}) {
  useFocusOnRemove(bar, entry.id)
  const Icon = entry.icon
  const content = (
    <>
      {entry.dot !== undefined && (
        <span
          aria-hidden="true"
          className={`size-1.5 shrink-0 rounded-full ${entry.dot === 'live' ? 'bg-add' : 'bg-signal'}`}
        />
      )}
      {entry.busy === true && <Spinner size={ICON.sm} />}
      {Icon !== undefined && <Icon size={ICON.sm} aria-hidden="true" className="shrink-0" />}
      {!iconOnly && <span>{entry.label}</span>}
    </>
  )
  const className = `flex h-6 shrink-0 items-center gap-1.5 rounded-md px-1.5 text-xs whitespace-nowrap ${
    entry.tone === 'signal' ? 'text-signal' : 'text-muted'
  }`
  if (entry.menu !== undefined) {
    return (
      <BarMenu
        entry={entry}
        menu={entry.menu}
        content={content}
        className={className}
        iconOnly={iconOnly}
      />
    )
  }
  if (entry.onSelect === undefined) {
    // A status, not a control.
    return (
      <span
        role="status"
        aria-label={iconOnly ? entry.label : undefined}
        data-item={entry.id}
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
      data-item={entry.id}
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
  jobs,
  onOpenTask,
  onFilterRunning,
  onNeedsYou,
  onRepos,
  onImport,
  notifications,
  onShortcuts,
  onSettings,
  onSignOut,
  width,
}: StatusBarProps) {
  const bar = useRef<HTMLElement>(null)
  const [level, setLevel] = useState(0)
  const [barWidth, setBarWidth] = useState(0)
  const lastKey = useRef('')
  const jobsText = jobsLabel(jobs)

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
    ...(jobsText === null
      ? []
      : [
          {
            id: 'jobs',
            priority: 4,
            label: jobsText,
            tip: 'Show background jobs',
            busy: true,
            menu: (now: Date) => jobRows(jobs, now, onOpenTask),
          },
        ]),
    {
      id: 'repos',
      priority: 6,
      label: 'Projects',
      tip: 'Projects',
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
    // The jobs show that something runs, so they never hide in the menu.
    folds: entry.id !== 'jobs',
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

  const left = entries.filter((entry) => LEFT.includes(entry.id))
  const right = entries.filter((entry) => !LEFT.includes(entry.id))
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
      className="flex h-7 shrink-0 items-center gap-0.5 overflow-hidden border-t border-line bg-shell px-1.5"
    >
      {left
        .filter((entry) => !folded.has(entry.id))
        .map((entry) => (
          <BarItem key={entry.id} entry={entry} iconOnly={iconOnly.has(entry.id)} bar={bar} />
        ))}
      <span className="flex-1" />
      {right
        .filter((entry) => !folded.has(entry.id))
        .map((entry) => (
          <BarItem key={entry.id} entry={entry} iconOnly={iconOnly.has(entry.id)} bar={bar} />
        ))}
      {overflow.length > 0 && (
        <Menu label="More status items" items={overflow} icon={Ellipsis} tipSide="top" />
      )}
      <IconButton
        label="Keyboard shortcuts"
        icon={Keyboard}
        shortcut={shortcutOf('help.shortcuts')}
        size="xs"
        tipSide="top"
        onClick={onShortcuts}
      />
      <AccountMenu onSignOut={onSignOut} onShortcuts={onShortcuts} onSettings={onSettings} />
    </footer>
  )
}
