import type { DiffStat, Task } from '@helmsman/core'
import { Copy, FileDiff, FolderMinus, GitBranch, Pencil, Pin, PinOff } from 'lucide-react'
import { useEffect, useRef, useState } from 'react'
import { formatAge, isQuiet } from '../lib/tasks'
import { toast } from '../lib/toasts'
import { shortBranch } from '../lib/workspace-groups'
import { StatusGlyph } from './StatusGlyph'
import { ConfirmDialog } from './ui/Dialog'
import { Menu } from './ui/Menu'
import { input } from './ui/styles'

type WorkspaceRowProps = {
  task: Task
  now: Date
  current: boolean
  pinned: boolean
  stat?: DiffStat
  // What the agent does now, for a running task.
  activity?: string
  onOpen: (id: string) => void
  onViewChanges: (id: string) => void
  onRename: (id: string, title: string) => void
  onTogglePin: (id: string) => void
  onRemove: (id: string) => void
}

function copy(text: string, what: string) {
  navigator.clipboard
    ?.writeText(text)
    .then(() => toast(`Copied the ${what}.`))
    .catch(() => toast(`Could not copy the ${what}.`, 'error'))
}

// One workspace in the sidebar: its state, title and age, then its branch and the size of its
// work, then what its agent does. Double-click the title to rename it.
export function WorkspaceRow({
  task,
  now,
  current,
  pinned,
  stat,
  activity,
  onOpen,
  onViewChanges,
  onRename,
  onTogglePin,
  onRemove,
}: WorkspaceRowProps) {
  const [renaming, setRenaming] = useState(false)
  const [title, setTitle] = useState(task.title)
  const [removing, setRemoving] = useState(false)
  const field = useRef<HTMLInputElement>(null)
  // Escape removes the field, which then fires a blur: this stops that blur from keeping the name.
  const editing = useRef(false)
  const finished = task.state === 'done' || task.state === 'cancelled'

  useEffect(() => {
    if (renaming) {
      field.current?.select()
    }
  }, [renaming])

  const startRename = () => {
    editing.current = true
    setTitle(task.title)
    setRenaming(true)
  }

  // Enter and leaving the field keep the name; Escape drops it.
  const finishRename = (keep: boolean) => {
    if (!editing.current) {
      return
    }
    editing.current = false
    setRenaming(false)
    if (keep && title.trim() !== '' && title.trim() !== task.title) {
      onRename(task.id, title.trim())
    }
  }

  return (
    <li className="group relative">
      {renaming ? (
        <form
          onSubmit={(event) => {
            event.preventDefault()
            finishRename(true)
          }}
          className="px-1 py-0.5"
        >
          <input
            ref={field}
            value={title}
            onChange={(event) => setTitle(event.target.value)}
            onBlur={() => finishRename(true)}
            onKeyDown={(event) => {
              if (event.key === 'Escape') {
                event.stopPropagation()
                finishRename(false)
              }
            }}
            aria-label={`New name for ${task.id}`}
            className={`${input} w-full`}
          />
        </form>
      ) : (
        <button
          type="button"
          onClick={() => onOpen(task.id)}
          onDoubleClick={startRename}
          aria-current={current}
          className={`flex w-full flex-col gap-0.5 rounded-lg py-1.5 pr-8 pl-2 text-left pointer-coarse:py-3 hover:bg-hover aria-[current=true]:bg-selected ${finished ? 'opacity-60' : ''}`}
        >
          <span className="flex w-full items-center gap-2">
            <StatusGlyph state={task.state} quiet={isQuiet(task, now)} />
            <span className="min-w-0 flex-1 truncate font-medium">{task.title}</span>
          </span>
          <span className="flex w-full min-w-0 items-center gap-2 pl-6 text-xs text-muted">
            <span className="min-w-0 truncate font-mono">{shortBranch(task) || task.id}</span>
            {stat !== undefined && stat.files > 0 && (
              <span className="shrink-0 font-mono">
                <span className="text-add">+{stat.added}</span>{' '}
                <span className="text-remove">−{stat.removed}</span>
              </span>
            )}
            <span className="ml-auto shrink-0">
              {formatAge(task.updatedAt, now).replace(' ago', '')}
            </span>
          </span>
          {task.state === 'running' && activity !== undefined && (
            <span className="w-full truncate pl-6 text-xs text-muted/80">{activity}</span>
          )}
        </button>
      )}
      {!renaming && (
        <span className="absolute top-1.5 right-1.5 pointer-coarse:top-2">
          <Menu
            label={`Actions for ${task.id}`}
            revealOnHover
            items={[
              { label: 'View changes', icon: FileDiff, onSelect: () => onViewChanges(task.id) },
              {
                label: 'Rename',
                icon: Pencil,
                onSelect: startRename,
              },
              {
                label: pinned ? 'Unpin' : 'Pin to the top',
                icon: pinned ? PinOff : Pin,
                onSelect: () => onTogglePin(task.id),
              },
              {
                label: 'Copy branch name',
                icon: GitBranch,
                disabled: task.branch === null,
                onSelect: () => copy(task.branch ?? '', 'branch name'),
              },
              {
                label: 'Copy folder path',
                icon: Copy,
                onSelect: () => copy(task.workspace ?? '', 'folder path'),
              },
              {
                label: 'Remove workspace',
                icon: FolderMinus,
                danger: true,
                onSelect: () => setRemoving(true),
              },
            ]}
          />
        </span>
      )}
      {removing && (
        <ConfirmDialog
          title="Remove the workspace?"
          description={`This deletes the worktree folder of ${task.id}. The branch stays when it has commits that are not merged. The task stays too.`}
          confirmLabel="Remove"
          onConfirm={() => onRemove(task.id)}
          onClose={() => setRemoving(false)}
        />
      )}
    </li>
  )
}
