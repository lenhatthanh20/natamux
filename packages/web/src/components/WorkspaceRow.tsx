import type { DiffStat, Task, TaskState } from '@natamux/core'
import {
  ChevronRight,
  Copy,
  FileDiff,
  FolderMinus,
  GitBranch,
  GitPullRequest,
  ListTree,
  Pencil,
  Pin,
  PinOff,
  Trash2,
} from 'lucide-react'
import { type FocusEvent, type ReactNode, useEffect, useRef, useState } from 'react'
import { agentLabel } from '../lib/agents'
import { CHECKS_COLOR, pullRequestNumber, pullRequestSummary } from '../lib/pull-request'
import { type AgentChild, stagePosition } from '../lib/sidebar-tree'
import {
  deleteTaskDescription,
  formatAge,
  formatExact,
  isQuiet,
  laneSummary,
  rowLabel,
  STATE_LABEL,
  stageLabel,
} from '../lib/tasks'
import { toast } from '../lib/toasts'
import { AgentIcon } from './AgentBadge'
import { DiffSize } from './DiffSize'
import { StatusGlyph } from './StatusGlyph'
import { ConfirmDialog } from './ui/Dialog'
import { Menu, type MenuItem, openContextMenu } from './ui/Menu'
import { ICON } from './ui/styles'

type RovingTab = {
  tabIndex: number
  'data-roving': string
  onFocus: (event: FocusEvent<HTMLElement>) => void
}

// The line under a row that folds what works under it: its agents and open subtasks.
export type RowFold = {
  open: boolean
  onToggle: () => void
  text: string
  // The most urgent state under the row. Null: nothing open.
  state: TaskState | null
}

type WorkspaceRowProps = {
  task: Task
  now: Date
  current: boolean
  pinned: boolean
  // Pinned rows come from several repositories, so they name theirs.
  repoName?: string
  stat?: DiffStat
  // What the agent does now, for a running task.
  activity?: string
  // Why the task failed. Null while it is not known. The Needs you section has the question.
  question?: string | null
  // The state changed since the user last opened this workspace.
  unseen: boolean
  // The roving tab stop from the tree: tabIndex, data-roving and onFocus.
  tab: RovingTab
  // Done and all subtasks of the task. Null: it has none.
  subtasks?: { done: number; total: number } | null
  fold?: RowFold
  // The row was added after the list was first drawn, so it slides in.
  enter?: boolean
  // The rows under the open fold.
  children?: ReactNode
  onOpen: (id: string) => void
  onViewChanges: (id: string) => void
  onRename: (id: string, title: string) => void
  onTogglePin: (id: string) => void
  onRemove: (id: string) => void
  onDelete?: (id: string) => void
}

function copy(text: string, what: string) {
  navigator.clipboard
    ?.writeText(text)
    .then(() => toast(`Copied the ${what}.`))
    .catch(() => toast(`Could not copy the ${what}.`, 'error'))
}

const ROW =
  'flex min-h-11 w-full flex-col rounded-md px-2 py-1.5 text-left text-xs leading-4 pointer-coarse:min-h-14 pointer-coarse:justify-center'

// One workspace in the sidebar: its state, title and age, then its branch and the size of its
// work, then what its agent does or asks. Double-click the title, or press F2, to rename it.
export function WorkspaceRow({
  task,
  now,
  current,
  pinned,
  repoName,
  stat,
  activity,
  question,
  unseen,
  tab,
  subtasks = null,
  fold,
  enter = false,
  children,
  onOpen,
  onViewChanges,
  onRename,
  onTogglePin,
  onRemove,
  onDelete,
}: WorkspaceRowProps) {
  const [renaming, setRenaming] = useState(false)
  const [title, setTitle] = useState(task.title)
  const [removing, setRemoving] = useState(false)
  const [deleting, setDeleting] = useState(false)
  const [menuOpen, setMenuOpen] = useState(false)
  const field = useRef<HTMLInputElement>(null)
  const row = useRef<HTMLButtonElement>(null)
  // Escape removes the field, which then fires a blur: this stops that blur from keeping the name.
  const editing = useRef(false)
  // Focus goes back to the row after a rename, but not on first render.
  const wasRenaming = useRef(false)
  const finished = task.state === 'done' || task.state === 'cancelled'
  const quiet = isQuiet(task, now)
  // Only rows that come after the first paint slide in; the first paint stays still.
  const [slides] = useState(enter)
  // Each time the task starts to wait, the row flashes once.
  const [flash, setFlash] = useState(0)
  const lastState = useRef(task.state)
  useEffect(() => {
    if (task.state !== lastState.current && task.state === 'needs-input') {
      setFlash((count) => count + 1)
    }
    lastState.current = task.state
  }, [task.state])
  const stage = stageLabel(task)
  const position = stagePosition(task)

  useEffect(() => {
    if (renaming) {
      field.current?.focus()
      field.current?.select()
    } else if (wasRenaming.current) {
      row.current?.focus()
    }
    wasRenaming.current = renaming
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

  const items: MenuItem[] = [
    { label: 'View changes', icon: FileDiff, onSelect: () => onViewChanges(task.id) },
    { label: 'Rename', icon: Pencil, shortcut: 'F2', onSelect: startRename },
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
    { type: 'separator' },
    {
      label: 'Remove workspace',
      icon: FolderMinus,
      danger: true,
      onSelect: () => setRemoving(true),
    },
    ...(onDelete === undefined
      ? []
      : [
          {
            label: 'Delete task',
            icon: Trash2,
            danger: true,
            onSelect: () => setDeleting(true),
          },
        ]),
  ]

  const titleClass = `min-w-0 flex-1 truncate text-sm ${unseen ? 'font-semibold' : 'font-medium'} ${
    finished ? 'text-muted' : 'text-ink'
  } ${finished && task.state === 'cancelled' ? 'line-through' : ''}`
  const metaColor = 'text-muted'

  const body = (titleLine: ReactNode): ReactNode => (
    <>
      <span className="flex h-4 w-full items-center gap-2">
        <StatusGlyph state={task.state} quiet={quiet} tip={laneSummary(task)} />
        {titleLine}
        {unseen && (
          <span
            data-unseen
            className="size-1.5 shrink-0 rounded-full bg-accent"
            aria-hidden="true"
          />
        )}
        {!renaming && (
          <span
            data-tip={formatExact(task.updatedAt)}
            className={`shrink-0 ${metaColor} group-focus-within:invisible group-hover:invisible pointer-coarse:invisible ${menuOpen ? 'invisible' : ''}`}
          >
            {formatAge(task.updatedAt, now)}
          </span>
        )}
      </span>
      <span className={`flex w-full min-w-0 items-center gap-2 pl-6 ${metaColor}`}>
        {repoName !== undefined && (
          <span className="shrink-0 rounded-sm bg-badge px-1 text-2xs font-medium text-ink-2">
            {repoName}
          </span>
        )}
        {task.agent !== null && (
          <span data-tip={agentLabel(task.agent)} className="inline-flex shrink-0">
            <AgentIcon agent={task.agent} />
          </span>
        )}
        {/* The branch is mostly the title, so it waits in the tooltip and the menu. */}
        <span data-tip={task.branch ?? undefined} className="shrink-0 font-mono">
          {task.id}
        </span>
        {stage !== null && (
          <span className="min-w-0 truncate">
            {position === null ? stage : `${stage} ${position.at}/${position.of}`}
          </span>
        )}
        {task.pr !== null && (
          <span
            data-tip={`Pull request ${pullRequestSummary(task) ?? ''}`.trim()}
            className={`inline-flex shrink-0 items-center gap-0.5 ${
              task.prStatus === null ? '' : CHECKS_COLOR[task.prStatus.checks]
            }`}
          >
            <GitPullRequest size={ICON.xs} aria-hidden="true" />
            {pullRequestNumber(task.pr)}
          </span>
        )}
        {subtasks !== null && (
          <span data-tip="Subtasks done" className="inline-flex shrink-0 items-center gap-0.5">
            <ListTree size={ICON.xs} aria-hidden="true" />
            {`${subtasks.done}/${subtasks.total}`}
          </span>
        )}
        <span className="flex-1" />
        <DiffSize stat={stat} empty={null} />
      </span>
      {task.state === 'running' && activity !== undefined && activity !== '' && (
        <span className={`block h-4 w-full truncate pl-6 ${metaColor}`}>{activity}</span>
      )}
      {/* Why it failed, so the user need not open each failed task to find out. */}
      {task.state === 'failed' && typeof question === 'string' && (
        <span
          data-tip-overflow
          data-state="failed"
          className="block h-4 w-full truncate pl-6 text-(--chip-fg)"
        >
          {question}
        </span>
      )}
    </>
  )

  return (
    <li className={slides ? 'hm-pop' : undefined}>
      <div className="group relative">
        {flash > 0 && (
          <span
            key={flash}
            aria-hidden="true"
            className="sb-flash pointer-events-none absolute inset-0 rounded-md"
          />
        )}
        {renaming ? (
          <div className={ROW}>
            {body(
              <form
                onSubmit={(event) => {
                  event.preventDefault()
                  finishRename(true)
                }}
                className="min-w-0 flex-1"
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
                  className="h-6 w-full rounded-sm border border-focus bg-input px-1.5 text-sm text-ink outline-none"
                />
              </form>,
            )}
          </div>
        ) : (
          <button
            ref={row}
            type="button"
            {...tab}
            onClick={() => onOpen(task.id)}
            onContextMenu={(event) => openContextMenu(event, items, `Actions for ${task.id}`)}
            onKeyDown={(event) => {
              if (event.key === 'F2') {
                event.preventDefault()
                event.stopPropagation()
                startRename()
              } else if (event.key === 'Delete') {
                event.preventDefault()
                event.stopPropagation()
                setRemoving(true)
              } else if ((event.key === 'F10' && event.shiftKey) || event.key === 'ContextMenu') {
                event.stopPropagation()
                openContextMenu(
                  {
                    preventDefault: () => event.preventDefault(),
                    currentTarget: event.currentTarget,
                  },
                  items,
                  `Actions for ${task.id}`,
                )
              }
            }}
            aria-current={current ? 'true' : undefined}
            aria-label={`${rowLabel(task, now)}${unseen ? ', unseen' : ''}`}
            className={`${ROW} sb-row focus-inset hover:bg-hover aria-[current=true]:bg-selected group-focus-within/tree:aria-[current=true]:bg-selection-active`}
          >
            {body(
              // biome-ignore lint/a11y/noStaticElementInteractions: the row around it is the button; a double click on the title is a shortcut for F2.
              <span
                onDoubleClick={(event) => {
                  event.stopPropagation()
                  startRename()
                }}
                className={titleClass}
              >
                {task.title}
              </span>,
            )}
          </button>
        )}
        {!renaming && (
          <span className="absolute top-0.5 right-1 pointer-coarse:top-3">
            <Menu
              label={`Actions for ${task.id}`}
              revealOnHover
              onOpenChange={setMenuOpen}
              items={items}
            />
          </span>
        )}
      </div>
      {fold !== undefined && (
        <button
          type="button"
          // The row takes Left and Right for this, so the tree stays one tab stop.
          tabIndex={-1}
          aria-expanded={fold.open}
          aria-label={`Work under ${task.id}`}
          onClick={fold.onToggle}
          className="flex h-5 w-full items-center gap-1.5 rounded-md pr-2 pl-6 text-2xs text-muted hover:text-ink pointer-coarse:h-10"
        >
          <ChevronRight
            size={ICON.xs}
            aria-hidden="true"
            className={`sb-chevron shrink-0 ${fold.open ? 'rotate-90' : ''}`}
          />
          {fold.state !== null && (
            <span
              className="state-dot"
              style={{ width: 6, height: 6 }}
              data-state={fold.state}
              aria-hidden="true"
            />
          )}
          <span className="min-w-0 truncate">{fold.text}</span>
        </button>
      )}
      {fold?.open === true && children}
      {removing && (
        <ConfirmDialog
          title="Remove the workspace?"
          description={`This deletes the worktree folder of ${task.id}. The branch stays when it has commits that are not merged. The task stays too.`}
          confirmLabel="Remove"
          onConfirm={() => onRemove(task.id)}
          onClose={() => setRemoving(false)}
        />
      )}
      {deleting && onDelete !== undefined && (
        <ConfirmDialog
          title={`Delete ${task.id}?`}
          description={deleteTaskDescription(task)}
          confirmLabel="Delete task"
          onConfirm={() => onDelete(task.id)}
          onClose={() => setDeleting(false)}
        />
      )}
    </li>
  )
}

type AgentRowProps = {
  agent: AgentChild
  // What the agent of this shell does now.
  activity?: string
  tab: RovingTab
  onOpen: () => void
}

// One agent under a task: its shell or its parallel stage, its state and what it does.
export function AgentRow({ agent, activity, tab, onOpen }: AgentRowProps) {
  const label = [agent.name, agent.stage, STATE_LABEL[agent.state]].filter(Boolean).join(', ')
  return (
    <li>
      <button
        type="button"
        {...tab}
        aria-label={label}
        onClick={onOpen}
        className="focus-inset flex h-6.5 w-full items-center gap-2 rounded-md px-2 text-left text-xs hover:bg-hover pointer-coarse:h-10"
      >
        <StatusGlyph state={agent.state} />
        <span className="shrink-0 font-medium text-ink-2">{agent.name}</span>
        {agent.stage !== null && <span className="shrink-0 text-muted">{agent.stage}</span>}
        {activity !== undefined && activity !== '' && (
          <span className="min-w-0 flex-1 truncate text-muted">{activity}</span>
        )}
      </button>
    </li>
  )
}
