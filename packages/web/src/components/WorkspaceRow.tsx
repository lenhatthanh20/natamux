import type { Task, TaskState } from '@natamux/core'
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
import { type FocusEvent, type ReactNode, useEffect, useId, useRef, useState } from 'react'
import { agentLabel } from '../lib/agents'
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
import { AgentIcon } from './AgentBadge'
import { ShapeGlyph } from './ShapeGlyph'
import { ConfirmDialog } from './ui/Dialog'
import { Menu, type MenuItem, openContextMenu } from './ui/Menu'
import { ICON } from './ui/styles'

type RovingTab = {
  tabIndex: number
  'data-roving': string
  onFocus: (event: FocusEvent<HTMLElement>) => void
}

type WorkspaceRowProps = {
  task: Task
  now: Date
  current: boolean
  pinned: boolean
  // Pinned rows come from several repositories, so they name theirs.
  repoName?: string
  // What the task glyph shows: the task state, or running while one of its agents works.
  glyph: TaskState
  // What the agent does now, for a running task.
  activity?: string
  // What the agent asked, or why the task failed. Null while it is not known.
  question?: string | null
  // The state changed since the user last opened this workspace.
  unseen: boolean
  // The roving tab stop from the tree: tabIndex, data-roving and onFocus.
  tab: RovingTab
  // Done and all subtasks of the task. Null: it has none.
  subtasks?: { done: number; total: number } | null
  // Only the open workspace shows the rows under it: its shells and open subtasks.
  open?: boolean
  // It has rows under it, shown once it is the open workspace.
  expandable?: boolean
  // Opens the terminal of the agent that asked.
  onAnswer?: () => void
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

// One workspace in the sidebar: its state, title and age, then its id, step and what its agent
// does. A task that waits on the user shows the question; a finished one is a single line.
// Double-click the title, or press F2, to rename it.
export function WorkspaceRow({
  task,
  now,
  current,
  pinned,
  repoName,
  glyph,
  activity,
  question,
  unseen,
  tab,
  subtasks = null,
  open = false,
  expandable = false,
  onAnswer,
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
  const answers = task.state === 'needs-input' && onAnswer !== undefined
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
              {`${subtasks.done}/${subtasks.total}`}
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
      <div className="group relative">
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
            onClick={onAnswer}
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
}

type AgentRowProps = {
  row: ShellRow
  // What the agent of this shell does now.
  activity?: string
  tab: RovingTab
  onOpen: () => void
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
export function AgentRow({ row, activity, tab, onOpen }: AgentRowProps) {
  const tag = shellTag(row)
  return (
    <li>
      <button
        type="button"
        {...tab}
        aria-label={shellRowName(row)}
        onClick={onOpen}
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
}
