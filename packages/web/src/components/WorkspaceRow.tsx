import type { Task, TaskState } from '@natamux/core'
import { useQuery } from '@tanstack/react-query'
import {
  Copy,
  FileDiff,
  FolderMinus,
  GitBranch,
  GitPullRequest,
  ListTree,
  Pencil,
  Pin,
  PinOff,
  SquareTerminal,
  Trash2,
} from 'lucide-react'
import { memo, type ReactNode, useEffect, useId, useRef, useState } from 'react'
import { useActivity } from '../hooks/useActivity'
import { useNow } from '../hooks/useNow'
import { rovingItem } from '../hooks/useRovingFocus'
import { agentLabel } from '../lib/agents'
import type { StoredEvent } from '../lib/api'
import { CHECKS_COLOR, pullRequestNumber, pullRequestSummary } from '../lib/pull-request'
import { cometOf, rowTier, type ShellRow, shellRowName, shellTag } from '../lib/sidebar-tree'
import {
  deleteTaskDescription,
  formatAge,
  formatExact,
  isQuiet,
  laneSummary,
  rowLabel,
} from '../lib/tasks'
import { toast } from '../lib/toasts'
import { rowText, waitReason } from '../lib/workspace'
import { AgentIcon } from './AgentBadge'
import { ShapeGlyph } from './ShapeGlyph'
import { ConfirmDialog } from './ui/Dialog'
import { Menu, type MenuItem, openContextMenu } from './ui/Menu'
import { ICON } from './ui/styles'

type WorkspaceRowProps = {
  task: Task
  current: boolean
  pinned: boolean
  // Pinned rows come from several repositories, so they name theirs.
  repoName?: string
  // What the task glyph shows: the task state, or running while one of its agents works.
  glyph: TaskState
  // The state changed since the user last opened this workspace.
  unseen: boolean
  // The row has the one tab stop of the tree. `activate` is the tree's, to move the stop here.
  tabStop: boolean
  activate: (key: string) => void
  // Done of all subtasks, such as "2/5". Null: it has none.
  subtasks?: string | null
  // Only the open workspace shows the rows under it: its shells and open subtasks.
  open?: boolean
  // It has rows under it, shown once it is the open workspace.
  expandable?: boolean
  // Opens the terminal of a task on this shell. Null: its first shell. Answer uses it.
  onOpenShell?: (taskId: string, shell: string | null) => void
  // The shell of the agent that asked. Null: not known.
  answerShell?: string | null
  // The task's events, for what its agent asked or why it failed.
  loadEvents?: (taskId: string) => Promise<StoredEvent[]>
  // The row was added after the list was first drawn, so it slides in.
  enter?: boolean
  // The rows under it: all of them while open, else only the subtasks that wait on the user.
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
  'flex w-full flex-col justify-center rounded-md px-2.5 text-left text-xs leading-4 pointer-coarse:min-h-14'

// The height follows what the row needs: a card for what waits on the user, two lines for work
// that goes on, one line for what is finished.
const TIER = {
  attention: 'row-attn my-0.5 min-h-[68px] rounded-lg py-2',
  live: 'min-h-10 py-1.5',
  quiet: 'min-h-6 py-1',
} as const

// The height a row off screen counts with before it was first drawn, by tier.
const OFF_SCREEN = {
  attention: '[--cv-h:4.5rem]',
  live: '[--cv-h:2.5rem]',
  quiet: '[--cv-h:1.5rem]',
} as const

// The line under a card, from the task's events.
const questionOf = (events: StoredEvent[]) => rowText(waitReason(events))

// One workspace in the sidebar: its state, title and age, then its id, step and what its agent
// does. A task that waits on the user shows the question; a finished one is a single line.
// Double-click the title, or press F2, to rename it. Memoized: the tree renders it again only when
// its own props change, and it reads its activity and question itself.
export const WorkspaceRow = memo(function WorkspaceRow({
  task,
  current,
  pinned,
  repoName,
  glyph,
  unseen,
  tabStop,
  activate,
  subtasks = null,
  open = false,
  expandable = false,
  onOpenShell,
  answerShell = null,
  loadEvents,
  enter = false,
  children,
  onOpen,
  onViewChanges,
  onRename,
  onTogglePin,
  onRemove,
  onDelete,
}: WorkspaceRowProps) {
  // What the agent does now, for a running task.
  const activity = useActivity((all) => all.byTask[task.id])
  // What the agent asked, or why the task failed. Undefined while it is not known.
  const asks =
    (task.state === 'needs-input' || task.state === 'failed') &&
    task.workspace !== null &&
    loadEvents !== undefined
  // Only a row that asks follows each change of its task; the others keep one idle entry.
  const question = useQuery({
    queryKey: ['events', task.id, asks ? task.updatedAt : null],
    queryFn: () => loadEvents?.(task.id) ?? Promise.resolve<StoredEvent[]>([]),
    enabled: asks,
    select: questionOf,
  }).data
  const tab = rovingItem(`task:${task.id}`, tabStop, activate)
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
  const now = useNow()
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
  const comet = cometOf(task)
  const tier = rowTier(task.state)
  // Screen readers hear every step, as the comet's tooltip shows them.
  const stepsId = useId()

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

  const weight = unseen || tier === 'attention' ? 'font-semibold' : 'font-medium'
  const size = tier === 'quiet' ? 'text-xs font-normal' : `text-sm ${weight}`
  const tone = finished ? 'text-muted' : tier === 'quiet' ? 'text-ink-2' : 'text-ink'
  const titleClass = `min-w-0 flex-1 truncate ${size} ${tone} ${task.state === 'cancelled' ? 'line-through' : ''}`
  const metaColor = 'text-muted'
  const answers = task.state === 'needs-input' && onOpenShell !== undefined
  const step = comet.of > 1 ? `${comet.at}/${comet.of}` : null
  // What a card says: the question, or why the task failed or waits.
  const said = question ?? (task.state === 'needs-input' ? 'Waiting for your answer' : null)

  // The branch is mostly the title, so it waits in the tooltip and the menu.
  const id = (
    <span data-tip={task.branch ?? undefined} className="shrink-0 font-mono text-2xs">
      {task.id}
    </span>
  )
  const stage = (
    <>
      {step !== null && <span className="shrink-0 font-mono text-2xs tabular-nums">{step}</span>}
      {comet.stage !== null && <span className="min-w-0 shrink truncate">{comet.stage}</span>}
    </>
  )
  // The agent that drives the task. Open, the rows under it name the agent of each stage.
  const agent = task.agent !== null && (
    <span data-tip={agentLabel(task.agent)} className="inline-flex shrink-0">
      <AgentIcon agent={task.agent} />
    </span>
  )

  const body = (titleLine: ReactNode): ReactNode => (
    <>
      <span className="flex h-4 w-full items-center gap-2">
        <ShapeGlyph
          state={glyph}
          quiet={quiet}
          size={tier === 'attention' ? 9 : 8}
          tip={laneSummary(task)}
        />
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
      <span id={stepsId} className="sr-only">{`Steps: ${comet.summary}`}</span>
      {tier === 'live' && (
        <span className={`mt-0.5 flex w-full min-w-0 items-center gap-1.5 pl-5.5 ${metaColor}`}>
          {repoName !== undefined && (
            <span className="shrink-0 rounded-sm bg-badge px-1 text-2xs font-medium text-ink-2">
              {repoName}
            </span>
          )}
          {id}
          {stage}
          {subtasks !== null && (
            <span data-tip="Subtasks done" className="inline-flex shrink-0 items-center gap-0.5">
              <ListTree size={ICON.xs} aria-hidden="true" />
              {subtasks}
            </span>
          )}
          {/* Open, each shell row says what its agent does. */}
          <span className="min-w-0 flex-1 truncate text-ink-2">
            {task.state === 'running' && !open ? activity : null}
          </span>
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
          {agent}
        </span>
      )}
      {tier === 'attention' && said !== null && (
        <span data-tip-overflow className="mt-1 line-clamp-2 w-full pl-5.5 text-ink">
          {said}
        </span>
      )}
      {tier === 'attention' && (
        <span className={`mt-1.5 flex w-full min-w-0 items-center gap-1.5 pl-5.5 ${metaColor}`}>
          {/* Room for the Answer button, which sits on top of the row. */}
          {answers && <span className="h-5 w-14 shrink-0" />}
          {id}
          {stage}
          <span className="flex-1" />
          {agent}
        </span>
      )}
    </>
  )

  return (
    <li className={slides ? 'hm-pop' : undefined}>
      <div className={`group relative cv-row ${OFF_SCREEN[tier]}`}>
        {flash > 0 && (
          <span
            key={flash}
            aria-hidden="true"
            className="sb-flash pointer-events-none absolute inset-0 rounded-md"
          />
        )}
        {renaming ? (
          <div
            data-state={tier === 'attention' ? task.state : undefined}
            className={`${ROW} ${TIER[tier]}`}
          >
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
            aria-expanded={expandable ? open : undefined}
            aria-label={`${rowLabel(task, now)}${unseen ? ', unseen' : ''}`}
            aria-describedby={comet.of > 0 ? stepsId : undefined}
            // The card takes its tint from the state.
            data-state={tier === 'attention' ? task.state : undefined}
            className={`${ROW} ${TIER[tier]} sb-row focus-inset hover:bg-hover aria-[current=true]:bg-selected group-focus-within/tree:aria-[current=true]:bg-selection-active`}
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
        {answers && !renaming && (
          <button
            type="button"
            // The row keeps the one tab stop of the tree; Enter on it opens the task.
            tabIndex={-1}
            data-state={task.state}
            aria-label={`Answer ${task.id}`}
            onClick={() => onOpenShell?.(task.id, answerShell)}
            className="absolute bottom-2.5 left-8 h-5 rounded-md bg-[color-mix(in_srgb,var(--chip-fg)_15%,transparent)] px-1.5 text-2xs font-medium text-(--chip-fg) hover:bg-[color-mix(in_srgb,var(--chip-fg)_25%,transparent)]"
          >
            Answer
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
      {children}
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
})

type AgentRowProps = {
  taskId: string
  row: ShellRow
  tabStop: boolean
  activate: (key: string) => void
  // Opens the terminal of the task on this shell. Null: its first shell.
  onOpenShell: (taskId: string, shell: string | null) => void
}

// A shell with no agent shows a terminal; an agent with no state yet, a muted ring.
function RowGlyph({ row }: { row: ShellRow }) {
  if (row.plain) {
    return (
      <span className="inline-flex size-3.5 shrink-0 items-center justify-center text-muted">
        <SquareTerminal size={ICON.xs} aria-hidden="true" />
      </span>
    )
  }
  if (row.state === null) {
    return (
      <span
        data-tip="State not known yet"
        className="inline-flex size-3.5 shrink-0 items-center justify-center text-muted"
      >
        {/* The ring of the ready state at the size of the other shell glyphs, in muted. */}
        <svg width={9} height={9} viewBox="-1 -1 9 9" aria-hidden="true">
          <circle cx={3.5} cy={3.5} r={2.75} fill="none" stroke="currentColor" strokeWidth={1.5} />
        </svg>
      </span>
    )
  }
  return <ShapeGlyph state={row.state} size={7} />
}

// One shell under a task, or a parallel stage with no shell yet: its state, its agent, what it
// works on and does, and its shell.
export const AgentRow = memo(function AgentRow({
  taskId,
  row,
  tabStop,
  activate,
  onOpenShell,
}: AgentRowProps) {
  const tag = shellTag(row)
  // What the agent of this shell does now.
  const activity = useActivity((all) =>
    row.shell === null ? undefined : all.byTerminal[taskId]?.[row.shell],
  )
  return (
    <li>
      <button
        type="button"
        {...rovingItem(`agent:${taskId}:${row.key}`, tabStop, activate)}
        aria-label={shellRowName(row)}
        onClick={() => onOpenShell(taskId, row.shell)}
        className="focus-inset flex h-5.5 w-full items-center gap-2 rounded-md px-2 text-left text-xs hover:bg-hover pointer-coarse:h-10"
      >
        <RowGlyph row={row} />
        {row.agent !== null && (
          <span data-tip={agentLabel(row.agent)} className="inline-flex shrink-0">
            <AgentIcon agent={row.agent} />
          </span>
        )}
        <span className={`shrink-0 font-medium ${row.plain ? 'text-muted' : 'text-ink-2'}`}>
          {row.label}
        </span>
        <span className="min-w-0 flex-1 truncate text-muted">{activity}</span>
        {tag !== null && (
          <span data-tip={`Shell ${row.shell}`} className="shrink-0 font-mono text-2xs text-muted">
            {tag}
          </span>
        )}
      </button>
    </li>
  )
})
