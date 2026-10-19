import type { Repo, Task, TaskState } from '@natamux/core'
import { useQueries } from '@tanstack/react-query'
import {
  ChevronRight,
  Eye,
  EyeOff,
  FolderMinus,
  FolderPlus,
  Pin,
  Plus,
  Settings,
} from 'lucide-react'
import { type CSSProperties, useEffect, useRef, useState } from 'react'
import { CountBadge } from '../../components/ui/CountBadge'
import { ConfirmDialog } from '../../components/ui/Dialog'
import { IconButton } from '../../components/ui/IconButton'
import type { MenuItem } from '../../components/ui/Menu'
import { ICON, sectionTitle } from '../../components/ui/styles'
import { useRovingFocus } from '../../hooks/useRovingFocus'
import { useStableCallback } from '../../hooks/useStableCallback'
import type { StoredEvent, TaskShells } from '../../lib/api'
import { shortcutOf } from '../../lib/commands'
import { NO_SHELLS, taskShellsQuery } from '../../lib/task-shells'
import { isUnseen } from '../../lib/view-store/helpers'
import {
  DEFAULT_REPO_ID,
  type Hold,
  heldId,
  nextHold,
  workspaceGroups,
} from '../../lib/workspace-groups'
import { ClearFinishedDialog } from './ClearFinishedDialog'
import { projectColor } from './project-color'
import { RepoHeader } from './RepoHeader'
import { BLURRED_KEY, isFinished, savedBlurred, savedFolds } from './sidebar-prefs'
import {
  FOLDS_KEY,
  isFoldOpen,
  rowTier,
  shellRows,
  subtaskCount,
  taskGlyphState,
  withFold,
  withParents,
} from './sidebar-tree'
import { treeEntries, treeKey } from './tree-entries'
import { AgentRow, WorkspaceRow } from './WorkspaceRow'

type WorkspaceListProps = {
  repos: readonly Pick<Repo, 'id' | 'name' | 'path'>[]
  // The workspaces to show: the filter is already applied.
  tasks: readonly Task[]
  // Every task, to count what the filter hides in each repository.
  allTasks: readonly Task[]
  // The filter text. Empty: no filter.
  filter?: string
  currentId: string | null
  pinnedIds: readonly string[]
  // A task's events: a row that waits on the user reads what the agent asked, or why it failed.
  loadEvents?: (taskId: string) => Promise<StoredEvent[]>
  // The state each task had when the user last opened it.
  seenState?: Readonly<Record<string, TaskState>>
  // The shells of a task, read while its agents work or wait, and for the open workspace.
  loadTerminals?: (taskId: string) => Promise<TaskShells>
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

// A task whose agents work or wait has its shells read, so its rows show without a click.
const ACTIVE: readonly TaskState[] = ['running', 'needs-input']

// The guide sits under the task glyph; the rows' glyphs line up with the title.
const GUIDE = 'ml-4 flex flex-col border-l border-line pl-[7px]'

// Every workspace, grouped by repository, like folders in an editor's sidebar. The whole tree is
// one tab stop: arrows move, Left and Right fold and unfold, F2 renames. The rows are memoized and
// read their activity themselves, so a change of one task renders only its row.
export function WorkspaceList({
  repos,
  tasks,
  allTasks,
  filter = '',
  currentId,
  pinnedIds,
  loadEvents,
  seenState = {},
  loadTerminals,
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
  // The same functions on every render, so the memoized rows keep their props.
  const openTask = useStableCallback(onOpen)
  const openShell = useStableCallback(onOpenShell)
  const viewChanges = useStableCallback(onViewChanges)
  const rename = useStableCallback(onRename)
  const togglePin = useStableCallback(onTogglePin)
  const remove = useStableCallback(onRemove)
  const deleteTask = useStableCallback((id: string) => onDelete?.(id))
  const readEvents = useStableCallback(async (id: string) => (await loadEvents?.(id)) ?? [])
  const [folds, setFolds] = useState(savedFolds)
  const [blurred, setBlurred] = useState(savedBlurred)
  const [removing, setRemoving] = useState<Pick<Repo, 'id' | 'name' | 'path'> | null>(null)
  // The repository whose finished workspaces the user asked to remove.
  const [clearing, setClearing] = useState<string | null>(null)
  // False during the first paint, so only what comes later slides in.
  const painted = useRef(false)
  useEffect(() => {
    painted.current = true
  }, [])
  // Read and kept during render, so the row stays put in the same render that finishes the task.
  const hold = useRef<Hold>({ id: null, open: false })
  hold.current = nextHold(hold.current, currentId, allTasks)
  const { pinned, groups, children } = workspaceGroups(
    repos,
    withParents(tasks, allTasks),
    pinnedIds,
    heldId(hold.current),
  )
  const finishedIn = (id: string) => groups.find((group) => group.repo.id === id)?.finished ?? []
  const repoName = (task: Task) =>
    repos.find((repo) => repo.id === (task.repo ?? 'default'))?.name ?? task.repo ?? undefined

  const filtered = filter.trim() !== ''
  const totalIn = (repoId: string) =>
    allTasks.filter((task) => task.workspace !== null && (task.repo ?? DEFAULT_REPO_ID) === repoId)
      .length

  const toggleBlur = (id: string) => {
    const next = blurred.includes(id) ? blurred.filter((item) => item !== id) : [...blurred, id]
    setBlurred(next)
    try {
      globalThis.localStorage?.setItem(BLURRED_KEY, JSON.stringify(next))
    } catch {
      // Not kept; the next visit shows the names.
    }
  }
  // Labels show as tooltips, so a blurred project is not named in them.
  const nameOf = (repo: Pick<Repo, 'id' | 'name'>) =>
    blurred.includes(repo.id) ? 'this project' : repo.name
  const blurItem = (id: string): MenuItem =>
    blurred.includes(id)
      ? { label: 'Show names', icon: Eye, onSelect: () => toggleBlur(id) }
      : { label: 'Blur names', icon: EyeOff, onSelect: () => toggleBlur(id) }

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
      return [blurItem(id), ...clearItems(id)]
    }
    const items: MenuItem[] = [
      { label: 'Start a task', icon: Plus, onSelect: () => onNewWorkspace(id) },
      { label: 'Settings', icon: Settings, onSelect: () => onRepoSettings(id) },
      blurItem(id),
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
    queries: loading.map((id) =>
      taskShellsQuery(id, (taskId) => loadTerminals?.(taskId) ?? Promise.resolve(NO_SHELLS)),
    ),
  })
  const rowsOf = (task: Task) => shellRows(task, terminalQueries[loading.indexOf(task.id)]?.data)
  const canFold = (task: Task) => (children[task.id]?.length ?? 0) > 0 || rowsOf(task).length > 0
  const taskOpen = (task: Task) => canFold(task) && holdsCurrent(task)
  // A folded row still shows the subtasks that wait on the user: no other part of the sidebar does.
  const waitingChildren = (task: Task) =>
    (children[task.id] ?? []).filter((child) => rowTier(child.state) === 'attention')

  const entries = treeEntries({
    pinned,
    groups,
    children,
    repoOpen,
    finishedShown,
    taskOpen,
    canFold,
    rowsOf,
    waitingChildren,
    setFold,
    onOpen,
  })

  const roving = useRovingFocus({
    keys: entries.map((entry) => entry.key),
    labels: entries.map((entry) => entry.label),
    preferred: currentId === null ? null : `task:${currentId}`,
    onKey: (event, key) => treeKey(event, key, entries, (target) => roving.focusKey(target)),
  })

  const row = (task: Task, prefix = false, nested = false) => {
    const open = !nested && taskOpen(task)
    const waiting = open ? [] : waitingChildren(task)
    const key = `task:${task.id}`
    const subtasks = subtaskCount(task.id, allTasks)
    // Only rows with something under them get children, so the others keep equal props.
    const under = open ? (
      <ul className={`${GUIDE} ${painted.current ? 'sb-unfold' : ''}`}>
        {rowsOf(task).map((shell) => (
          <AgentRow
            key={shell.key}
            taskId={task.id}
            row={shell}
            tabStop={roving.stop === `agent:${task.id}:${shell.key}`}
            activate={roving.activate}
            onOpenShell={openShell}
          />
        ))}
        {(children[task.id] ?? []).map((child) => row(child, false, true))}
      </ul>
    ) : waiting.length > 0 ? (
      <ul className={GUIDE}>{waiting.map((child) => row(child, false, true))}</ul>
    ) : undefined
    return (
      <WorkspaceRow
        key={task.id}
        task={task}
        current={task.id === currentId}
        pinned={pinnedIds.includes(task.id)}
        repoName={prefix ? repoName(task) : undefined}
        blurred={prefix && blurred.includes(task.repo ?? DEFAULT_REPO_ID)}
        glyph={taskGlyphState(task, rowsOf(task))}
        unseen={isUnseen(seenState, task)}
        tabStop={roving.stop === key}
        activate={roving.activate}
        subtasks={subtasks === null ? null : `${subtasks.done}/${subtasks.total}`}
        enter={painted.current}
        open={open}
        expandable={!nested && canFold(task)}
        onOpenShell={openShell}
        answerShell={rowsOf(task).find((shell) => shell.state === 'needs-input')?.shell ?? null}
        loadEvents={loadEvents === undefined ? undefined : readEvents}
        onOpen={openTask}
        onViewChanges={viewChanges}
        onRename={rename}
        onTogglePin={togglePin}
        onRemove={remove}
        onDelete={onDelete === undefined ? undefined : deleteTask}
      >
        {under}
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
        const shownFinished = finishedShown(repo.id)
        const hidden = totalIn(repo.id) - open.length - finished.length - nested.length
        const hiddenInGroup = filtered && open.length + finished.length === 0 && hidden > 0
        const items = repoItems(repo.id)
        const isBlurred = blurred.includes(repo.id)
        const name = nameOf(repo)
        const menuLabel = `Actions for ${name}`
        // A project that is gone takes no new tasks.
        const gone = !repos.some((known) => known.id === repo.id)
        // With the gap of the list, 8px between projects.
        const spaced = index > 0 ? 'mt-1' : ''
        return (
          // A card per project, in the project's color, so each one reads as a block.
          <div
            key={repo.id}
            style={{ '--project': projectColor(repo.id) } as CSSProperties}
            data-blurred={isBlurred || undefined}
            className={`flex flex-col rounded-[10px] border border-line bg-[color-mix(in_srgb,var(--surface)_55%,transparent)] ${spaced}`}
          >
            <RepoHeader
              group={group}
              count={count}
              isCollapsed={isCollapsed}
              items={items}
              menuLabel={menuLabel}
              name={name}
              isBlurred={isBlurred}
              gone={gone}
              currentId={currentId}
              itemProps={roving.item(`repo:${repo.id}`)}
              onToggle={() => setFold(`repo:${repo.id}`, isCollapsed)}
              onToggleBlur={() => toggleBlur(repo.id)}
              onNewWorkspace={onNewWorkspace}
              onRepoSettings={onRepoSettings}
            />
            {!isCollapsed && (
              <ul className="flex flex-col px-0.5 pt-0.5 pb-1">
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
                  <li data-private className="py-1 pl-8 text-xs text-muted">
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
