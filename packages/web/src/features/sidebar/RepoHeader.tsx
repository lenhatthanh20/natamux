import { ChevronRight, Eye, EyeOff, FolderGit2, Plus, Settings } from 'lucide-react'
import { CountBadge } from '../../components/ui/CountBadge'
import { IconButton } from '../../components/ui/IconButton'
import { Menu, type MenuItem, openContextMenu } from '../../components/ui/Menu'
import { ICON } from '../../components/ui/styles'
import type { rovingItem } from '../../hooks/useRovingFocus'
import type { RepoGroup } from '../../lib/workspace-groups'
import { HiddenDot } from './HiddenDot'

// The row of a project card: fold button, quick actions and the menu.
export function RepoHeader({
  group,
  count,
  isCollapsed,
  items,
  menuLabel,
  name,
  isBlurred,
  gone,
  currentId,
  itemProps,
  onToggle,
  onToggleBlur,
  onNewWorkspace,
  onRepoSettings,
}: {
  group: RepoGroup
  count: number
  isCollapsed: boolean
  items: MenuItem[]
  menuLabel: string
  name: string
  isBlurred: boolean
  // A project that is gone takes no new tasks.
  gone: boolean
  currentId: string | null
  itemProps: ReturnType<typeof rovingItem>
  onToggle: () => void
  onToggleBlur: () => void
  onNewWorkspace: (repo?: string) => void
  onRepoSettings: (id: string) => void
}) {
  const { repo } = group
  return (
    <div
      className={`group flex h-7 items-center bg-linear-to-r from-[color-mix(in_srgb,var(--project)_16%,transparent)] to-transparent to-85% pr-0.5 hover:from-[color-mix(in_srgb,var(--project)_20%,transparent)] hover:to-[color-mix(in_srgb,var(--project)_4%,transparent)] ${isCollapsed ? 'rounded-[9px]' : 'rounded-t-[9px]'}`}
    >
      <button
        type="button"
        {...itemProps}
        aria-expanded={!isCollapsed}
        onClick={onToggle}
        onContextMenu={(event) => {
          if (items.length > 0) {
            openContextMenu(event, items, menuLabel)
          }
        }}
        onKeyDown={(event) => {
          const menuKey = (event.key === 'F10' && event.shiftKey) || event.key === 'ContextMenu'
          if (menuKey && items.length > 0) {
            event.stopPropagation()
            openContextMenu(
              {
                preventDefault: () => event.preventDefault(),
                currentTarget: event.currentTarget,
              },
              items,
              menuLabel,
            )
          }
        }}
        className="focus-inset flex min-w-0 flex-1 items-center gap-1.5 self-stretch rounded-md px-1.5 text-left"
      >
        <ChevronRight
          size={ICON.xs}
          aria-hidden="true"
          className={`sb-chevron shrink-0 text-muted ${isCollapsed ? '' : 'rotate-90'}`}
        />
        <FolderGit2 size={ICON.sm} aria-hidden="true" className="shrink-0 text-(--project)" />
        <span data-private className="min-w-0 truncate text-sm font-semibold text-ink">
          {repo.name}
        </span>
        <CountBadge
          count={count}
          tone="project"
          label={`${count} workspace${count === 1 ? '' : 's'}`}
        />
        {isCollapsed && <HiddenDot group={group} currentId={currentId} repoName={name} />}
      </button>
      {!gone && (
        <IconButton
          label={`Start a task in ${name}`}
          icon={Plus}
          size="xs"
          onClick={() => onNewWorkspace(repo.id)}
          className="opacity-0 group-focus-within:opacity-100 group-hover:opacity-100 pointer-coarse:opacity-100"
        />
      )}
      {!gone && (
        <IconButton
          label={`Settings of ${name}`}
          icon={Settings}
          size="xs"
          onClick={() => onRepoSettings(repo.id)}
          className="opacity-0 group-focus-within:opacity-100 group-hover:opacity-100 pointer-coarse:opacity-100"
        />
      )}
      <IconButton
        label={isBlurred ? 'Show names' : 'Blur names'}
        icon={isBlurred ? Eye : EyeOff}
        size="xs"
        aria-pressed={isBlurred}
        onClick={onToggleBlur}
        // Always on screen while blurred, so the user sees why the names are gone.
        className={
          isBlurred
            ? ''
            : 'opacity-0 group-focus-within:opacity-100 group-hover:opacity-100 pointer-coarse:opacity-100'
        }
      />
      {items.length > 0 && <Menu label={menuLabel} items={items} revealOnHover />}
    </div>
  )
}
