import type { DiffStat, Repo, Task } from '@helmsman/core'
import { ChevronRight, FolderGit2, FolderPlus, Pin, Plus } from 'lucide-react'
import { useState } from 'react'
import { workspaceGroups } from '../lib/workspace-groups'
import { iconButton, sectionTitle } from './ui/styles'
import { WorkspaceRow } from './WorkspaceRow'

type WorkspaceListProps = {
  repos: readonly Pick<Repo, 'id' | 'name'>[]
  tasks: readonly Task[]
  now: Date
  currentId: string | null
  pinnedIds: readonly string[]
  diffStats?: Readonly<Record<string, DiffStat>>
  activity?: Readonly<Record<string, string>>
  onOpen: (id: string) => void
  onViewChanges: (id: string) => void
  onRename: (id: string, title: string) => void
  onTogglePin: (id: string) => void
  onRemove: (id: string) => void
  // Opens the dialog for a new workspace, in that repository when one is given.
  onNewWorkspace: (repo?: string) => void
  onAddRepo: () => void
}

// Every workspace, grouped by repository, like folders in an editor's sidebar.
export function WorkspaceList({
  repos,
  tasks,
  now,
  currentId,
  pinnedIds,
  diffStats = {},
  activity = {},
  onOpen,
  onViewChanges,
  onRename,
  onTogglePin,
  onRemove,
  onNewWorkspace,
  onAddRepo,
}: WorkspaceListProps) {
  const [collapsed, setCollapsed] = useState<ReadonlySet<string>>(new Set())
  const { pinned, groups } = workspaceGroups(repos, tasks, pinnedIds)

  const toggle = (id: string) =>
    setCollapsed((current) => {
      const next = new Set(current)
      if (!next.delete(id)) {
        next.add(id)
      }
      return next
    })

  const row = (task: Task) => (
    <WorkspaceRow
      key={task.id}
      task={task}
      now={now}
      current={task.id === currentId}
      pinned={pinnedIds.includes(task.id)}
      stat={diffStats[task.id]}
      activity={activity[task.id]}
      onOpen={onOpen}
      onViewChanges={onViewChanges}
      onRename={onRename}
      onTogglePin={onTogglePin}
      onRemove={onRemove}
    />
  )

  return (
    <section aria-labelledby="workspaces-heading" className="flex flex-col gap-1">
      <div className="flex h-7 items-center gap-0.5 pr-0.5 pl-2">
        <h2 id="workspaces-heading" className={`flex-1 ${sectionTitle}`}>
          Workspaces
        </h2>
        <button
          type="button"
          aria-label="Add a repository"
          title="Add a repository"
          onClick={onAddRepo}
          className={`${iconButton} size-6`}
        >
          <FolderPlus size={14} aria-hidden="true" />
        </button>
        <button
          type="button"
          aria-label="Add a workspace"
          title="Add a workspace (c)"
          onClick={() => onNewWorkspace()}
          className={`${iconButton} size-6`}
        >
          <Plus size={15} aria-hidden="true" />
        </button>
      </div>

      {pinned.length > 0 && (
        <div className="flex flex-col">
          <h3 className="flex h-7 items-center gap-1.5 px-2 text-xs text-muted">
            <Pin size={12} aria-hidden="true" />
            Pinned
          </h3>
          <ul className="flex flex-col">{pinned.map(row)}</ul>
        </div>
      )}

      {groups.map(({ repo, open, finished }) => {
        const isCollapsed = collapsed.has(repo.id)
        return (
          <div key={repo.id} className="flex flex-col">
            <div className="group flex h-7 items-center rounded-md pr-0.5 hover:bg-hover">
              <button
                type="button"
                aria-expanded={!isCollapsed}
                onClick={() => toggle(repo.id)}
                className="flex min-w-0 flex-1 items-center gap-1.5 self-stretch px-1.5 text-left"
              >
                <ChevronRight
                  size={13}
                  aria-hidden="true"
                  className={`shrink-0 text-muted transition-transform ${isCollapsed ? '' : 'rotate-90'}`}
                />
                <FolderGit2 size={14} aria-hidden="true" className="shrink-0 text-muted" />
                <span className="min-w-0 truncate font-medium">{repo.name}</span>
                <span className="text-xs text-muted">{open.length}</span>
              </button>
              <button
                type="button"
                aria-label={`New workspace in ${repo.name}`}
                title={`New workspace in ${repo.name}`}
                onClick={() => onNewWorkspace(repo.id)}
                className={`${iconButton} size-6 opacity-0 group-hover:opacity-100 focus-visible:opacity-100`}
              >
                <Plus size={14} aria-hidden="true" />
              </button>
            </div>
            {!isCollapsed && (
              <ul className="flex flex-col">
                {open.map(row)}
                {open.length === 0 && (
                  <li className="py-1 pl-8 text-xs text-muted">
                    No workspaces.{' '}
                    <button
                      type="button"
                      onClick={() => onNewWorkspace(repo.id)}
                      className="text-ink underline-offset-2 hover:underline"
                    >
                      Make one
                    </button>
                  </li>
                )}
                {finished.length > 0 && (
                  <li>
                    <details className="group/finished">
                      <summary className="flex h-6 cursor-default list-none items-center gap-1 pl-6 text-xs text-muted hover:text-ink">
                        <ChevronRight
                          size={12}
                          aria-hidden="true"
                          className="transition-transform group-open/finished:rotate-90"
                        />
                        {finished.length} finished
                      </summary>
                      <ul className="flex flex-col">{finished.map(row)}</ul>
                    </details>
                  </li>
                )}
              </ul>
            )}
          </div>
        )
      })}
    </section>
  )
}
