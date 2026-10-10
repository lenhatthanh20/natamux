import type { DiffStat, Repo, Task, TaskState } from '@natamux/core'
import {
  ChevronRight,
  FolderGit2,
  FolderMinus,
  FolderPlus,
  Pin,
  Plus,
  Settings,
} from 'lucide-react'
import { useState } from 'react'
import { useRovingFocus } from '../hooks/useRovingFocus'
import { shortcutOf } from '../lib/commands'
import { firstChildKey, parentKey, type TreeRow } from '../lib/roving'
import { isUnseen } from '../lib/view-store'
import { collapsedHint, DEFAULT_REPO_ID, workspaceGroups } from '../lib/workspace-groups'
import { CountBadge } from './ui/CountBadge'
import { ConfirmDialog } from './ui/Dialog'
import { IconButton } from './ui/IconButton'
import { Menu, type MenuItem, openContextMenu } from './ui/Menu'
import { ICON, sectionTitle } from './ui/styles'
import { WorkspaceRow } from './WorkspaceRow'

type WorkspaceListProps = {
  repos: readonly Pick<Repo, 'id' | 'name' | 'path'>[]
  // The workspaces to show: the filter is already applied.
  tasks: readonly Task[]
  // Every task, to count what the filter hides in each repository.
  allTasks: readonly Task[]
  // The filter text. Empty: no filter.
  filter?: string
  now: Date
  currentId: string | null
  pinnedIds: readonly string[]
  diffStats?: Readonly<Record<string, DiffStat>>
  activity?: Readonly<Record<string, string>>
  // What waiting agents asked, by task id.
  questions?: Readonly<Record<string, string | null>>
  // The state each task had when the user last opened it.
  seenState?: Readonly<Record<string, TaskState>>
  onOpen: (id: string) => void
  onViewChanges: (id: string) => void
  onRename: (id: string, title: string) => void
  onTogglePin: (id: string) => void
  onRemove: (id: string) => void
  // Removes the workspaces of these tasks, one after the other.
  onRemoveFinished: (ids: string[]) => void
  // A removal of finished workspaces runs.
  removingFinished?: boolean
  // Opens the dialog for a new workspace, in that repository when one is given.
  onNewWorkspace: (repo?: string) => void
  onAddRepo: () => void
  onRepoSettings: (id: string) => void
  onRemoveRepo: (id: string) => void
}

type Entry = TreeRow & { label: string }

// Every workspace, grouped by repository, like folders in an editor's sidebar. The whole tree is
// one tab stop: arrows move, Left and Right fold and unfold, F2 renames.
export function WorkspaceList({
  repos,
  tasks,
  allTasks,
  filter = '',
  now,
  currentId,
  pinnedIds,
  diffStats = {},
  activity = {},
  questions = {},
  seenState = {},
  onOpen,
  onViewChanges,
  onRename,
  onTogglePin,
  onRemove,
  onRemoveFinished,
  removingFinished = false,
  onNewWorkspace,
  onAddRepo,
  onRepoSettings,
  onRemoveRepo,
}: WorkspaceListProps) {
  const [collapsed, setCollapsed] = useState<ReadonlySet<string>>(new Set())
  const [removing, setRemoving] = useState<Pick<Repo, 'id' | 'name' | 'path'> | null>(null)
  const [finishedOpen, setFinishedOpen] = useState<ReadonlySet<string>>(new Set())
  // The repository whose finished workspaces the user asked to remove.
  const [clearing, setClearing] = useState<string | null>(null)
  const { pinned, groups } = workspaceGroups(repos, tasks, pinnedIds)
  const finishedIn = (id: string) => groups.find((group) => group.repo.id === id)?.finished ?? []
  const repoName = (task: Task) =>
    repos.find((repo) => repo.id === (task.repo ?? 'default'))?.name ?? task.repo ?? undefined

  const filtered = filter.trim() !== ''
  const totalIn = (repoId: string) =>
    allTasks.filter((task) => task.workspace !== null && (task.repo ?? DEFAULT_REPO_ID) === repoId)
      .length

  const clearItems = (id: string): MenuItem[] =>
    finishedIn(id).length === 0
      ? []
      : [
          { type: 'separator' },
          {
            label: 'Remove finished workspaces',
            icon: FolderMinus,
            danger: true,
            disabled: removingFinished,
            onSelect: () => setClearing(id),
          },
        ]

  // A group of a repository that is gone (its tasks are still there) has only the actions on its
  // workspaces.
  const repoItems = (id: string): MenuItem[] => {
    const known = repos.find((repo) => repo.id === id)
    if (known === undefined) {
      return clearItems(id).slice(1)
    }
    const items: MenuItem[] = [
      { label: 'Start a task', icon: Plus, onSelect: () => onNewWorkspace(id) },
      { label: 'Settings', icon: Settings, onSelect: () => onRepoSettings(id) },
      ...clearItems(id),
    ]
    if (id === DEFAULT_REPO_ID) {
      return items
    }
    // Every workspace counts, also the ones the filter hides: the daemon refuses them all.
    const busy = totalIn(id) > 0
    return [
      ...items,
      { type: 'separator' },
      {
        label: 'Remove project',
        icon: FolderMinus,
        danger: true,
        disabled: busy,
        hint: busy ? 'Remove its workspaces first' : undefined,
        onSelect: () => setRemoving(known),
      },
    ]
  }

  const flip = (set: ReadonlySet<string>, id: string) => {
    const next = new Set(set)
    if (!next.delete(id)) {
      next.add(id)
    }
    return next
  }

  // The rows as they are on screen, for the keyboard.
  const entries: Entry[] = []
  for (const task of pinned) {
    entries.push({ key: `task:${task.id}`, parent: null, expandable: false, label: task.title })
  }
  for (const { repo, open, finished } of groups) {
    const repoKey = `repo:${repo.id}`
    entries.push({ key: repoKey, parent: null, expandable: true, label: repo.name })
    if (collapsed.has(repo.id)) {
      continue
    }
    for (const task of open) {
      entries.push({
        key: `task:${task.id}`,
        parent: repoKey,
        expandable: false,
        label: task.title,
      })
    }
    if (finished.length > 0) {
      const finishedKey = `finished:${repo.id}`
      entries.push({ key: finishedKey, parent: repoKey, expandable: true, label: '' })
      if (finishedOpen.has(repo.id)) {
        for (const task of finished) {
          entries.push({
            key: `task:${task.id}`,
            parent: finishedKey,
            expandable: false,
            label: task.title,
          })
        }
      }
    }
  }

  const roving = useRovingFocus({
    keys: entries.map((entry) => entry.key),
    labels: entries.map((entry) => entry.label),
    preferred: currentId === null ? null : `task:${currentId}`,
    onKey: (event, key) => {
      const [kind, id] = [key.slice(0, key.indexOf(':')), key.slice(key.indexOf(':') + 1)]
      const isOpen =
        kind === 'repo' ? !collapsed.has(id) : kind === 'finished' ? finishedOpen.has(id) : false
      const expandable = kind === 'repo' || kind === 'finished'
      const toggleEntry = () => {
        if (kind === 'repo') {
          setCollapsed(flip(collapsed, id))
        } else {
          setFinishedOpen(flip(finishedOpen, id))
        }
      }
      if (event.key === 'ArrowRight' && expandable) {
        if (!isOpen) {
          toggleEntry()
        } else {
          const child = firstChildKey(entries, key)
          if (child !== null) {
            roving.focusKey(child)
          }
        }
        return true
      }
      if (event.key === 'ArrowLeft') {
        if (expandable && isOpen) {
          toggleEntry()
          return true
        }
        const parent = parentKey(entries, key)
        if (parent !== null) {
          roving.focusKey(parent)
          return true
        }
      }
      return false
    },
  })

  const row = (task: Task, prefix = false) => (
    <WorkspaceRow
      key={task.id}
      task={task}
      now={now}
      current={task.id === currentId}
      pinned={pinnedIds.includes(task.id)}
      repoName={prefix ? repoName(task) : undefined}
      stat={diffStats[task.id]}
      activity={activity[task.id]}
      question={questions[task.id]}
      unseen={isUnseen(seenState, task)}
      tab={roving.item(`task:${task.id}`)}
      onOpen={onOpen}
      onViewChanges={onViewChanges}
      onRename={onRename}
      onTogglePin={onTogglePin}
      onRemove={onRemove}
    />
  )

  return (
    <section
      ref={roving.container}
      aria-labelledby="workspaces-heading"
      {...roving.containerProps}
      className="group/tree flex flex-col gap-1"
    >
      <div className="sticky top-0 z-(--z-sticky) flex h-7 items-center gap-0.5 bg-sidebar pr-0.5 pl-2">
        <h2 id="workspaces-heading" className={`flex-1 ${sectionTitle}`}>
          Workspaces
        </h2>
        <IconButton label="Add a project" icon={FolderPlus} size="xs" onClick={onAddRepo} />
        <IconButton
          label="Start a task"
          icon={Plus}
          size="xs"
          shortcut={shortcutOf('workspace.new')}
          onClick={() => onNewWorkspace()}
        />
      </div>

      {pinned.length > 0 && (
        <div className="flex flex-col">
          <div className="flex h-7 items-center gap-1.5 px-2">
            <Pin size={ICON.xs} aria-hidden="true" className="text-muted" />
            <h3 className={sectionTitle}>Pinned</h3>
            <CountBadge count={pinned.length} />
          </div>
          <ul className="flex flex-col">{pinned.map((task) => row(task, true))}</ul>
        </div>
      )}

      {groups.map((group) => {
        const { repo, open, finished } = group
        const isCollapsed = collapsed.has(repo.id)
        const hint = isCollapsed ? collapsedHint(group, currentId, now) : null
        const shownFinished = finishedOpen.has(repo.id)
        const hidden = totalIn(repo.id) - open.length - finished.length
        const hiddenInGroup = filtered && open.length + finished.length === 0 && hidden > 0
        const items = repoItems(repo.id)
        const menuLabel = `Actions for ${repo.name}`
        return (
          <div key={repo.id} className="flex flex-col">
            <div className="group flex h-7 items-center rounded-md pr-0.5 hover:bg-hover">
              <button
                type="button"
                {...roving.item(`repo:${repo.id}`)}
                aria-expanded={!isCollapsed}
                onClick={() => setCollapsed(flip(collapsed, repo.id))}
                onContextMenu={(event) => {
                  if (items.length > 0) {
                    openContextMenu(event, items, menuLabel)
                  }
                }}
                onKeyDown={(event) => {
                  const menuKey =
                    (event.key === 'F10' && event.shiftKey) || event.key === 'ContextMenu'
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
                  className={`shrink-0 text-muted transition-transform ${isCollapsed ? '' : 'rotate-90'}`}
                />
                <FolderGit2 size={ICON.sm} aria-hidden="true" className="shrink-0 text-muted" />
                <span className="min-w-0 truncate font-medium">{repo.name}</span>
                <CountBadge count={open.length} />
                {hint !== null && (
                  <span
                    className="state-dot"
                    style={{ width: 6, height: 6 }}
                    data-state={hint}
                    data-tip={`A workspace in ${repo.name} is hidden here`}
                    aria-hidden="true"
                  />
                )}
              </button>
              <IconButton
                label={`Start a task in ${repo.name}`}
                icon={Plus}
                size="xs"
                onClick={() => onNewWorkspace(repo.id)}
                className="opacity-0 group-focus-within:opacity-100 group-hover:opacity-100 pointer-coarse:opacity-100"
              />
              {items.length > 0 && <Menu label={menuLabel} items={items} revealOnHover />}
            </div>
            {!isCollapsed && (
              <ul className="flex flex-col">
                {open.map((task) => row(task))}
                {open.length === 0 && !hiddenInGroup && (
                  <li className="py-1 pl-8 text-xs text-muted">
                    No workspaces.{' '}
                    <button
                      type="button"
                      onClick={() => onNewWorkspace(repo.id)}
                      className="text-ink underline-offset-2 hover:underline"
                    >
                      Start a task
                    </button>
                  </li>
                )}
                {hiddenInGroup && (
                  <li className="py-1 pl-8 text-xs text-muted">
                    No matches in {repo.name} ({hidden} hidden)
                  </li>
                )}
                {finished.length > 0 && (
                  <li>
                    <div className="group flex items-center pr-0.5">
                      <button
                        type="button"
                        {...roving.item(`finished:${repo.id}`)}
                        aria-expanded={shownFinished}
                        onClick={() => setFinishedOpen(flip(finishedOpen, repo.id))}
                        className="focus-inset flex h-6 min-w-0 flex-1 items-center gap-1 rounded-md pl-6 text-left text-xs text-muted hover:text-ink"
                      >
                        <ChevronRight
                          size={ICON.xs}
                          aria-hidden="true"
                          className={`transition-transform ${shownFinished ? 'rotate-90' : ''}`}
                        />
                        {finished.length} finished
                      </button>
                      <IconButton
                        label={`Remove finished workspaces in ${repo.name}`}
                        icon={FolderMinus}
                        size="xs"
                        disabled={removingFinished}
                        onClick={() => setClearing(repo.id)}
                        className="opacity-0 group-focus-within:opacity-100 group-hover:opacity-100 pointer-coarse:opacity-100"
                      />
                    </div>
                    {shownFinished && (
                      <ul className="flex flex-col">{finished.map((task) => row(task))}</ul>
                    )}
                  </li>
                )}
              </ul>
            )}
          </div>
        )
      })}

      {clearing !== null && (
        <ClearFinishedDialog
          repoName={groups.find((group) => group.repo.id === clearing)?.repo.name ?? clearing}
          tasks={finishedIn(clearing)}
          onConfirm={onRemoveFinished}
          onClose={() => setClearing(null)}
        />
      )}

      {removing !== null && (
        <ConfirmDialog
          title={`Remove ${removing.name} from Natamux?`}
          description={`Natamux forgets this project. The folder ${removing.path} and its branches stay on disk. Tasks without a workspace keep their link and come back if you add the folder again.`}
          confirmLabel="Remove"
          onConfirm={() => onRemoveRepo(removing.id)}
          onClose={() => setRemoving(null)}
        />
      )}
    </section>
  )
}

type ClearFinishedDialogProps = {
  repoName: string
  tasks: readonly Task[]
  onConfirm: (ids: string[]) => void
  onClose: () => void
}

function ClearFinishedDialog({ repoName, tasks, onConfirm, onClose }: ClearFinishedDialogProps) {
  const count = tasks.length === 1 ? '1 finished workspace' : `${tasks.length} finished workspaces`
  return (
    <ConfirmDialog
      title={`Remove ${count}?`}
      description={`This deletes the worktree folders of these tasks in ${repoName}. A branch stays when it has commits that are not merged. The tasks stay too. A workspace with changes that are not committed is skipped.`}
      confirmLabel="Remove"
      onConfirm={() => onConfirm(tasks.map((task) => task.id))}
      onClose={onClose}
    >
      <ul className="flex max-h-40 flex-col gap-0.5 overflow-y-auto text-xs">
        {tasks.map((task) => (
          <li key={task.id} className="flex min-w-0 gap-2">
            <span className="shrink-0 font-mono text-muted">{task.id}</span>
            <span className="min-w-0 truncate">{task.title}</span>
          </li>
        ))}
      </ul>
    </ConfirmDialog>
  )
}
