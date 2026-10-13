import type { Repo, Task, TaskState } from '@natamux/core'
import { useQueries } from '@tanstack/react-query'
import {
  ChevronRight,
  FolderGit2,
  FolderMinus,
  FolderPlus,
  Pin,
  Plus,
  Settings,
} from 'lucide-react'
import { useEffect, useRef, useState } from 'react'
import { useRovingFocus } from '../hooks/useRovingFocus'
import type { TaskShells } from '../lib/api'
import { shortcutOf } from '../lib/commands'
import { firstChildKey, parentKey, type TreeRow } from '../lib/roving'
import {
  FOLDS_KEY,
  isFoldOpen,
  parseFolds,
  rowTier,
  shellRows,
  subtaskCount,
  taskGlyphState,
  withFold,
  withParents,
} from '../lib/sidebar-tree'
import { isUnseen } from '../lib/view-store'
import { collapsedHint, DEFAULT_REPO_ID, workspaceGroups } from '../lib/workspace-groups'
import { CountBadge } from './ui/CountBadge'
import { ConfirmDialog } from './ui/Dialog'
import { IconButton } from './ui/IconButton'
import { Menu, type MenuItem, openContextMenu } from './ui/Menu'
import { ICON, sectionTitle } from './ui/styles'
import { AgentRow, WorkspaceRow } from './WorkspaceRow'

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
  activity?: Readonly<Record<string, string>>
  // What waiting agents asked, or why tasks failed, by task id.
  questions?: Readonly<Record<string, string | null>>
  // The state each task had when the user last opened it.
  seenState?: Readonly<Record<string, TaskState>>
  // The shells of a task, read while its agents work or wait, and for the open workspace.
  loadTerminals?: (taskId: string) => Promise<TaskShells>
  // What the agent in each shell does now, by task id and shell name.
  terminalActivity?: Readonly<Record<string, Readonly<Record<string, string>>>>
  onOpen: (id: string) => void
  // Opens the terminal of a task on this shell. Null: its first shell.
  onOpenShell: (taskId: string, shell: string | null) => void
  onViewChanges: (id: string) => void
  onRename: (id: string, title: string) => void
  onTogglePin: (id: string) => void
  onRemove: (id: string) => void
  onDelete?: (id: string) => void
  // Removes the workspaces of these tasks, one after the other.
  onRemoveFinished: (ids: string[]) => void
  // A removal of finished workspaces runs.
  removingFinished?: boolean
  // Opens the dialog for a new workspace, in that repository when one is given.
  onNewWorkspace: (repo?: string) => void
  onAddRepo: () => void
  // The daemon has no project: the list offers to add one.
  noProjects?: boolean
  onRepoSettings: (id: string) => void
  onRemoveRepo: (id: string) => void
}

type Entry = TreeRow & { label: string; open?: boolean; toggle?: () => void }

// Shells are read again this often while a row shows them, as the agents grid does.
const TERMINALS_EVERY_MS = 5000

const NO_SHELLS: TaskShells = { terminals: [], lanes: {}, programs: {} }

// A task whose agents work or wait has its shells read, so its rows show without a click.
const ACTIVE: readonly TaskState[] = ['running', 'needs-input']

function savedFolds(): Record<string, boolean> {
  try {
    return parseFolds(globalThis.localStorage?.getItem(FOLDS_KEY))
  } catch {
    return {}
  }
}

const isFinished = (task: Task) => task.state === 'done' || task.state === 'cancelled'

// The guide sits under the task glyph; the rows' glyphs line up with the title.
const GUIDE = 'ml-4 flex flex-col border-l border-line pl-[7px]'

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
  activity = {},
  questions = {},
  seenState = {},
  loadTerminals,
  terminalActivity = {},
  onOpen,
  onOpenShell,
  onViewChanges,
  onRename,
  onTogglePin,
  onRemove,
  onDelete,
  onRemoveFinished,
  removingFinished = false,
  onNewWorkspace,
  onAddRepo,
  noProjects = false,
  onRepoSettings,
  onRemoveRepo,
}: WorkspaceListProps) {
  const [folds, setFolds] = useState(savedFolds)
  const [removing, setRemoving] = useState<Pick<Repo, 'id' | 'name' | 'path'> | null>(null)
  // The repository whose finished workspaces the user asked to remove.
  const [clearing, setClearing] = useState<string | null>(null)
  // False during the first paint, so only what comes later slides in.
  const painted = useRef(false)
  useEffect(() => {
    painted.current = true
  }, [])
  const { pinned, groups, children } = workspaceGroups(
    repos,
    withParents(tasks, allTasks),
    pinnedIds,
  )
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

  // Only a choice of the user is kept, so a first visit writes nothing.
  const setFold = (key: string, open: boolean) => {
    const next = withFold(folds, key, open)
    setFolds(next)
    try {
      globalThis.localStorage?.setItem(FOLDS_KEY, JSON.stringify(next))
    } catch {
      // Not kept; the next visit starts with the defaults.
    }
  }
  const repoOpen = (id: string) => isFoldOpen(folds, `repo:${id}`, true)
  const finishedShown = (id: string) => isFoldOpen(folds, `finished:${id}`, false)

  // Only the open workspace shows what works under it, or its parent when it is a subtask. A
  // filter shows the subtasks it matched.
  const holdsCurrent = (task: Task) =>
    task.id === currentId ||
    (children[task.id] ?? []).some((child) => child.id === currentId || filtered)

  const shownRows = [
    ...pinned,
    ...groups.flatMap(({ repo, open, finished }) =>
      repoOpen(repo.id) ? [...open, ...(finishedShown(repo.id) ? finished : [])] : [],
    ),
  ]
  // The shells are read while its agents work or wait, and for the open workspace. A finished
  // task, or one without a workspace, has none left.
  const readsShells = (task: Task) =>
    !isFinished(task) &&
    task.workspace !== null &&
    (holdsCurrent(task) || ACTIVE.includes(task.state))
  const loading =
    loadTerminals === undefined ? [] : shownRows.filter(readsShells).map((task) => task.id)
  const terminalQueries = useQueries({
    queries: loading.map((id) => ({
      queryKey: ['task-shells', id],
      queryFn: () => loadTerminals?.(id) ?? Promise.resolve(NO_SHELLS),
      refetchInterval: TERMINALS_EVERY_MS,
    })),
  })
  const rowsOf = (task: Task) => shellRows(task, terminalQueries[loading.indexOf(task.id)]?.data)
  const canFold = (task: Task) => (children[task.id]?.length ?? 0) > 0 || rowsOf(task).length > 0
  const taskOpen = (task: Task) => canFold(task) && holdsCurrent(task)
  // A folded row still shows the subtasks that wait on the user: no other part of the sidebar does.
  const waitingChildren = (task: Task) =>
    (children[task.id] ?? []).filter((child) => rowTier(child.state) === 'attention')

  // The rows as they are on screen, for the keyboard.
  const entries: Entry[] = []
  const pushTask = (task: Task, parent: string | null) => {
    const key = `task:${task.id}`
    const open = taskOpen(task)
    entries.push({
      key,
      parent,
      expandable: canFold(task),
      label: task.title,
      open,
      // A folded task opens when it becomes the open workspace; the open one does not fold.
      toggle: open ? undefined : () => onOpen(task.id),
    })
    if (open) {
      for (const shell of rowsOf(task)) {
        entries.push({
          key: `agent:${task.id}:${shell.key}`,
          parent: key,
          expandable: false,
          label: shell.label,
        })
      }
    }
    for (const child of open ? (children[task.id] ?? []) : waitingChildren(task)) {
      entries.push({ key: `task:${child.id}`, parent: key, expandable: false, label: child.title })
    }
  }
  for (const task of pinned) {
    pushTask(task, null)
  }
  for (const { repo, open, finished } of groups) {
    const repoKey = `repo:${repo.id}`
    const shown = repoOpen(repo.id)
    entries.push({
      key: repoKey,
      parent: null,
      expandable: true,
      label: repo.name,
      open: shown,
      toggle: () => setFold(repoKey, !shown),
    })
    if (!shown) {
      continue
    }
    for (const task of open) {
      pushTask(task, repoKey)
    }
    if (finished.length > 0) {
      const finishedKey = `finished:${repo.id}`
      const finishedOpen = finishedShown(repo.id)
      entries.push({
        key: finishedKey,
        parent: repoKey,
        expandable: true,
        label: '',
        open: finishedOpen,
        toggle: () => setFold(finishedKey, !finishedOpen),
      })
      if (finishedOpen) {
        for (const task of finished) {
          pushTask(task, finishedKey)
        }
      }
    }
  }

  const roving = useRovingFocus({
    keys: entries.map((entry) => entry.key),
    labels: entries.map((entry) => entry.label),
    preferred: currentId === null ? null : `task:${currentId}`,
    onKey: (event, key) => {
      const entry = entries.find((candidate) => candidate.key === key)
      const expandable = entry?.expandable === true
      const isOpen = entry?.open === true
      if (event.key === 'ArrowRight' && expandable) {
        if (!isOpen) {
          entry?.toggle?.()
        } else {
          const child = firstChildKey(entries, key)
          if (child !== null) {
            roving.focusKey(child)
          }
        }
        return true
      }
      if (event.key === 'ArrowLeft') {
        if (expandable && isOpen && entry?.toggle !== undefined) {
          entry?.toggle?.()
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

  const row = (task: Task, prefix = false, nested = false) => {
    const open = !nested && taskOpen(task)
    const waiting = open ? [] : waitingChildren(task)
    return (
      <WorkspaceRow
        key={task.id}
        task={task}
        now={now}
        current={task.id === currentId}
        pinned={pinnedIds.includes(task.id)}
        repoName={prefix ? repoName(task) : undefined}
        glyph={taskGlyphState(task, rowsOf(task))}
        activity={activity[task.id]}
        question={questions[task.id]}
        unseen={isUnseen(seenState, task)}
        tab={roving.item(`task:${task.id}`)}
        subtasks={subtaskCount(task.id, allTasks)}
        enter={painted.current}
        open={open}
        expandable={!nested && canFold(task)}
        onAnswer={() =>
          onOpenShell(
            task.id,
            rowsOf(task).find((shell) => shell.state === 'needs-input')?.shell ?? null,
          )
        }
        onOpen={onOpen}
        onViewChanges={onViewChanges}
        onRename={onRename}
        onTogglePin={onTogglePin}
        onRemove={onRemove}
        onDelete={onDelete}
      >
        {open && (
          <ul className={`${GUIDE} ${painted.current ? 'sb-unfold' : ''}`}>
            {rowsOf(task).map((shell) => (
              <AgentRow
                key={shell.key}
                row={shell}
                activity={
                  shell.shell === null ? undefined : terminalActivity[task.id]?.[shell.shell]
                }
                tab={roving.item(`agent:${task.id}:${shell.key}`)}
                onOpen={() => onOpenShell(task.id, shell.shell)}
              />
            ))}
            {(children[task.id] ?? []).map((child) => row(child, false, true))}
          </ul>
        )}
        {waiting.length > 0 && (
          <ul className={GUIDE}>{waiting.map((child) => row(child, false, true))}</ul>
        )}
      </WorkspaceRow>
    )
  }

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

      {groups.map((group, index) => {
        const { repo, open, finished, nested } = group
        const count = open.length + nested.length
        const isCollapsed = !repoOpen(repo.id)
        const hint = isCollapsed ? collapsedHint(group, currentId, now) : null
        const shownFinished = finishedShown(repo.id)
        const hidden = totalIn(repo.id) - open.length - finished.length - nested.length
        const hiddenInGroup = filtered && open.length + finished.length === 0 && hidden > 0
        const items = repoItems(repo.id)
        const menuLabel = `Actions for ${repo.name}`
        // A project that is gone takes no new tasks.
        const gone = !repos.some((known) => known.id === repo.id)
        // With the gap of the list, 8px between projects.
        const spaced = index > 0 ? 'mt-1' : ''
        return (
          <div key={repo.id} className={`flex flex-col ${spaced}`}>
            {/* A band, so each project reads as a block above its rows. */}
            <div className="group flex h-7 items-center rounded-md bg-[color-mix(in_srgb,var(--ink)_5%,transparent)] pr-0.5 hover:bg-[color-mix(in_srgb,var(--ink)_8%,transparent)]">
              <button
                type="button"
                {...roving.item(`repo:${repo.id}`)}
                aria-expanded={!isCollapsed}
                onClick={() => setFold(`repo:${repo.id}`, isCollapsed)}
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
                  className={`sb-chevron shrink-0 text-muted ${isCollapsed ? '' : 'rotate-90'}`}
                />
                <FolderGit2 size={ICON.sm} aria-hidden="true" className="shrink-0 text-ink-2" />
                <span className="min-w-0 truncate text-sm font-semibold text-ink">{repo.name}</span>
                <CountBadge
                  count={count}
                  tone="accent"
                  label={`${count} workspace${count === 1 ? '' : 's'}`}
                />
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
              {!gone && (
                <IconButton
                  label={`Start a task in ${repo.name}`}
                  icon={Plus}
                  size="xs"
                  onClick={() => onNewWorkspace(repo.id)}
                  className="opacity-0 group-focus-within:opacity-100 group-hover:opacity-100 pointer-coarse:opacity-100"
                />
              )}
              {items.length > 0 && <Menu label={menuLabel} items={items} revealOnHover />}
            </div>
            {!isCollapsed && (
              <ul className="mt-0.5 flex flex-col">
                {open.map((task) => row(task))}
                {open.length === 0 && !hiddenInGroup && (
                  <li className="py-1 pl-8 text-xs text-muted">
                    No workspaces.{' '}
                    {!gone && (
                      <button
                        type="button"
                        onClick={() => onNewWorkspace(repo.id)}
                        className="text-ink underline-offset-2 hover:underline"
                      >
                        Start a task
                      </button>
                    )}
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
                        onClick={() => setFold(`finished:${repo.id}`, !shownFinished)}
                        className="focus-inset flex h-6 min-w-0 flex-1 items-center gap-1 rounded-md pl-6 text-left text-xs text-muted hover:text-ink"
                      >
                        <ChevronRight
                          size={ICON.xs}
                          aria-hidden="true"
                          className={`sb-chevron ${shownFinished ? 'rotate-90' : ''}`}
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

      {noProjects && groups.length === 0 && (
        <div className="flex items-center gap-2 py-1 pl-2 text-xs text-muted">
          <span>No projects yet</span>
          <button
            type="button"
            onClick={onAddRepo}
            className="text-ink underline-offset-2 hover:underline"
          >
            Add project
          </button>
        </div>
      )}

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
