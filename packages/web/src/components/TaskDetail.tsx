import { type DiffStat, portRange, type Task } from '@helmsman/core'
import {
  ArrowLeft,
  Ban,
  Copy,
  CopyPlus,
  ExternalLink,
  FileDiff,
  FolderMinus,
  Pencil,
  Play,
  RotateCcw,
  Save,
  X,
} from 'lucide-react'
import { useEffect, useState } from 'react'
import type { StoredEvent, TaskChanges } from '../lib/api'
import { copyText } from '../lib/clipboard'
import type { DetailsMode } from '../lib/layout-mode'
import { CHECKS_COLOR, pullRequestSummary } from '../lib/pull-request'
import {
  actionsFor,
  agentAction,
  formatAge,
  formatExact,
  isQuiet,
  type TaskAction,
} from '../lib/tasks'
import { CompareTries } from './CompareTries'
import { DetailSection, Field } from './DetailSection'
import { FanOutDialog } from './FanOutForm'
import { PathText } from './PathText'
import { StateBadge } from './StateBadge'
import { Subtasks } from './Subtasks'
import { LABEL_CHIP } from './TaskChips'
import { TaskEditForm } from './TaskEditForm'
import { Timeline } from './Timeline'
import { Button } from './ui/Button'
import { ConfirmDialog } from './ui/Dialog'
import { IconButton } from './ui/IconButton'
import { Menu, type MenuItem } from './ui/Menu'
import { Select } from './ui/Select'
import { ICON } from './ui/styles'

type TaskDetailProps = {
  task: Task
  now: Date
  busy: boolean
  // Docked beside the main area, a sheet over it, or a page that replaces it. The parent sizes it.
  mode?: DetailsMode
  loadEvents: () => Promise<StoredEvent[]>
  onAction: (action: TaskAction) => void
  onCreateWorkspace: (base?: string) => void
  // Local branches a workspace can start from, and the one the checkout is on.
  branches?: { branches: string[]; current: string | null }
  onRemoveWorkspace: () => void
  onViewChanges: () => void
  onStartAgent: () => void
  onEdit: (changes: TaskChanges) => void
  onCheckPullRequest: () => void
  // Tasks this one depends on that are not done yet.
  waitingFor?: readonly string[]
  onSaveTemplate?: () => void
  subtasks?: readonly Task[]
  onAddSubtask?: (title: string) => void
  onSelectTask?: (id: string) => void
  onFanOut?: (count: number, start: boolean) => void
  // The task and its tries from a fan-out, when it has some.
  tries?: readonly Task[]
  diffStats?: Readonly<Record<string, DiffStat>>
  onKeepTry?: (task: Task) => void
  onClose: () => void
}

// "tries-t-0002" is how the daemon groups tries. People read it as "Tries of T-0002".
export function labelText(label: string): string {
  const match = /^tries-(t-\d+)$/i.exec(label)
  return match?.[1] === undefined ? label : `Tries of ${match[1].toUpperCase()}`
}

function isTypingTarget(target: EventTarget | null): boolean {
  return (
    target instanceof Element &&
    (target.closest('input, textarea, select, [contenteditable="true"], .xterm, .monaco-editor') !==
      null)
  )
}

export function TaskDetail({
  task,
  now,
  busy,
  mode = 'docked',
  loadEvents,
  onAction,
  onCreateWorkspace,
  branches,
  onRemoveWorkspace,
  onViewChanges,
  onStartAgent,
  onEdit,
  onCheckPullRequest,
  waitingFor = [],
  onSaveTemplate,
  subtasks = [],
  onAddSubtask,
  onSelectTask,
  onFanOut,
  tries = [],
  diffStats,
  onKeepTry,
  onClose,
}: TaskDetailProps) {
  const [editing, setEditing] = useState(false)
  const [base, setBase] = useState('')
  const [asking, setAsking] = useState<'cancel' | 'remove' | 'tries' | null>(null)
  const actions = actionsFor(task.state)
  const stateActions = actions.filter((action) => !action.confirm)
  const cancelAction = actions.find((action) => action.confirm)
  const isFinished = task.state === 'done' || task.state === 'cancelled'
  const ports = portRange(task.id)
  const agentButton = agentAction(task)
  // Copies of a task that works now would run beside its own agent.
  const canFanOut = onFanOut !== undefined && !isFinished && agentButton !== null
  const page = mode === 'page'

  // E edits, when no field or terminal has the keys.
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (
        event.key === 'e' &&
        !event.metaKey &&
        !event.ctrlKey &&
        !event.altKey &&
        !isTypingTarget(event.target) &&
        !document.querySelector('[role="dialog"], [role="menu"]')
      ) {
        event.preventDefault()
        setEditing(true)
      }
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [])

  const menu: MenuItem[] = [
    ...(onSaveTemplate === undefined
      ? []
      : [{ label: 'Save as template', icon: Save, onSelect: onSaveTemplate }]),
    ...(canFanOut
      ? [{ label: 'Run more tries...', icon: CopyPlus, onSelect: () => setAsking('tries') }]
      : []),
    ...(cancelAction === undefined
      ? []
      : [
          ...(onSaveTemplate !== undefined || canFanOut ? [{ type: 'separator' as const }] : []),
          {
            label: cancelAction.label,
            icon: Ban,
            danger: true,
            onSelect: () => setAsking('cancel'),
          },
        ]),
  ]

  return (
    <aside
      aria-label={`Details of ${task.id}`}
      className="flex h-full w-full min-w-0 flex-col overflow-hidden bg-surface"
    >
      <header className={`flex h-10 shrink-0 items-center gap-1 border-b border-line pr-2 ${page ? 'pl-2' : 'pl-4'}`}>
        {page && <IconButton label="Back" icon={ArrowLeft} onClick={onClose} />}
        <span className="flex-1 font-mono text-xs text-muted">{task.id}</span>
        {!editing && <IconButton label="Edit" icon={Pencil} shortcut="e" onClick={() => setEditing(true)} />}
        {menu.length > 0 && <Menu label={`More actions for ${task.id}`} items={menu} size="sm" />}
        {!page && <IconButton label="Close details" icon={X} shortcut="Esc" onClick={onClose} />}
      </header>

      {editing ? (
        <TaskEditForm
          task={task}
          busy={busy}
          onSave={(changes) => {
            setEditing(false)
            onEdit(changes)
          }}
          onCancel={() => setEditing(false)}
        />
      ) : (
        <div className="flex min-h-0 flex-1 flex-col overflow-y-auto">
          <div className="flex flex-col gap-2 p-4">
            <h2 className="text-lg font-semibold tracking-tight break-words">{task.title}</h2>
            <div className="flex flex-wrap items-center gap-1.5">
              <StateBadge state={task.state} />
              {task.priority !== 'normal' && (
                <span className={`${LABEL_CHIP} capitalize`}>{task.priority}</span>
              )}
              {task.labels.map((label) => (
                <span key={label} className={LABEL_CHIP}>
                  {labelText(label)}
                </span>
              ))}
            </div>
          </div>

          {(isQuiet(task, now) || waitingFor.length > 0) && (
            <div className="flex flex-col gap-2 px-4 pb-4">
              {isQuiet(task, now) && (
                <p
                  role="status"
                  className="rounded-md border border-signal-line bg-signal-soft px-3 py-2.5 text-ink"
                >
                  No sign of life for {formatAge(task.updatedAt, now)}. The agent may be stuck.
                </p>
              )}
              {waitingFor.length > 0 && (
                <p className="rounded-md border border-signal-line bg-signal-soft px-3 py-2.5 text-ink">
                  Waits for {waitingFor.join(', ')} to be done
                </p>
              )}
            </div>
          )}

          {(stateActions.length > 0 || agentButton !== null) && (
            <div className="flex flex-col gap-1.5 px-4 pb-4">
              {page && <span className="text-2xs font-semibold tracking-[0.06em] text-muted uppercase">Actions</span>}
              <div className="flex flex-wrap items-center gap-2">
                {stateActions.map((action, index) => (
                  <Button
                    key={action.type}
                    variant={index === 0 ? 'primary' : 'secondary'}
                    disabled={busy}
                    shortcut={action.hotkey}
                    onClick={() => onAction(action)}
                  >
                    {action.label}
                  </Button>
                ))}
                {agentButton !== null && (
                  <Button
                    icon={agentButton === 'restart' ? RotateCcw : Play}
                    disabled={busy || waitingFor.length > 0}
                    onClick={onStartAgent}
                    data-tip={
                      waitingFor.length > 0
                        ? `Waits for ${waitingFor.join(', ')} to be done`
                        : agentButton === 'restart'
                          ? 'Starts Claude Code again in the terminal of the workspace'
                          : 'Creates the workspace when there is none, then starts Claude Code in its terminal'
                    }
                  >
                    {agentButton === 'restart' ? 'Restart agent' : 'Start agent'}
                  </Button>
                )}
              </div>
            </div>
          )}

          <DetailSection id="notes" title="Notes">
            {task.description !== '' ? (
              <p className="whitespace-pre-wrap text-ink-2">{task.description}</p>
            ) : (
              <p className="text-muted">No notes. Add some with Edit.</p>
            )}
          </DetailSection>

          <DetailSection id="workspace" title="Workspace">
            {task.workspace === null ? (
              <div className="flex flex-wrap items-center gap-2">
                {!isFinished && branches !== undefined && branches.branches.length > 1 && (
                  <Select
                    value={base}
                    onChange={(event) => setBase(event.target.value)}
                    aria-label="Start from branch"
                    className="w-40"
                  >
                    <option value="">{branches.current ?? 'HEAD'}</option>
                    {branches.branches
                      .filter((name) => name !== branches.current)
                      .map((name) => (
                        <option key={name} value={name}>
                          {name}
                        </option>
                      ))}
                  </Select>
                )}
                {isFinished ? (
                  <span className="text-muted">No workspace.</span>
                ) : (
                  <Button disabled={busy} onClick={() => onCreateWorkspace(base === '' ? undefined : base)}>
                    Create workspace
                  </Button>
                )}
              </div>
            ) : (
              <>
                <dl className="flex flex-col gap-0.5">
                  <Field
                    label="Branch"
                    action={
                      task.branch === null ? undefined : (
                        <IconButton
                          label="Copy the branch name"
                          icon={Copy}
                          size="xs"
                          onClick={() => copyText(task.branch ?? '', 'the branch name')}
                        />
                      )
                    }
                  >
                    {task.branch === null ? '-' : <PathText path={task.branch} />}
                  </Field>
                  <Field
                    label="Folder"
                    action={
                      <IconButton
                        label="Copy the folder path"
                        icon={Copy}
                        size="xs"
                        onClick={() => copyText(task.workspace ?? '', 'the folder path')}
                      />
                    }
                  >
                    <span className="text-xs">
                      <PathText path={task.workspace} />
                    </span>
                  </Field>
                  <Field label="Dev ports">
                    <span className="font-mono text-xs">
                      {ports.first}–{ports.last}
                    </span>
                    <span className="text-muted"> (first is $PORT)</span>
                  </Field>
                </dl>
                <div className="flex flex-wrap gap-2">
                  <Button icon={FileDiff} onClick={onViewChanges}>
                    View changes
                  </Button>
                  <Button variant="ghost" icon={FolderMinus} disabled={busy} onClick={() => setAsking('remove')}>
                    Remove workspace
                  </Button>
                </div>
              </>
            )}
          </DetailSection>

          {task.pr !== null && (
            <DetailSection id="pull-request" title="Pull request">
              <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
                <a
                  href={task.pr}
                  target="_blank"
                  rel="noreferrer"
                  className="inline-flex items-center gap-1 text-link hover:underline"
                >
                  {task.pr.replace(/^https:\/\/github\.com\//, '')}
                  <ExternalLink size={ICON.xs} aria-hidden="true" />
                </a>
                {task.prStatus !== null && (
                  <span className={CHECKS_COLOR[task.prStatus.checks]}>
                    {pullRequestSummary(task)}
                  </span>
                )}
                <Button variant="link" disabled={busy} onClick={onCheckPullRequest}>
                  Check now
                </Button>
              </div>
            </DetailSection>
          )}

          {task.parentId === null && onAddSubtask !== undefined && (
            <Subtasks
              subtasks={subtasks}
              busy={busy}
              onSelect={(id) => onSelectTask?.(id)}
              onAdd={onAddSubtask}
            />
          )}

          {tries.length > 1 && onKeepTry !== undefined && (
            <CompareTries
              tries={tries}
              currentId={task.id}
              diffStats={diffStats}
              busy={busy}
              onSelect={(id) => onSelectTask?.(id)}
              onKeep={onKeepTry}
            />
          )}

          <DetailSection id="details" title="Details" defaultOpen={false}>
            <dl className="flex flex-col gap-0.5">
              {task.repo !== null && <Field label="Repository">{task.repo}</Field>}
              <Field label="Depends on">
                {task.dependsOn.length > 0 ? task.dependsOn.join(', ') : '-'}
              </Field>
              {task.parentId !== null && (
                <Field label="Part of">
                  <button
                    type="button"
                    onClick={() => onSelectTask?.(task.parentId ?? '')}
                    className="font-mono text-xs text-link hover:underline"
                  >
                    {task.parentId}
                  </button>
                </Field>
              )}
              <Field label="Created">
                <span data-tip={formatExact(task.createdAt)}>
                  {formatAge(task.createdAt, now, 'long')}
                </span>
              </Field>
              <Field label="Updated">
                <span data-tip={formatExact(task.updatedAt)}>
                  {formatAge(task.updatedAt, now, 'long')}
                </span>
              </Field>
            </dl>
          </DetailSection>

          <Timeline taskId={task.id} updatedAt={task.updatedAt} now={now} loadEvents={loadEvents} />
        </div>
      )}

      {asking === 'tries' && onFanOut !== undefined && (
        <FanOutDialog busy={busy} onFanOut={onFanOut} onClose={() => setAsking(null)} />
      )}
      {asking === 'remove' && (
        <ConfirmDialog
          title="Remove the workspace?"
          description={`This deletes the worktree folder of ${task.id}. The branch stays when it has commits that are not merged.`}
          confirmLabel="Remove"
          onConfirm={onRemoveWorkspace}
          onClose={() => setAsking(null)}
        />
      )}
      {asking === 'cancel' && cancelAction !== undefined && (
        <ConfirmDialog
          title={`Cancel ${task.id}?`}
          description="A cancelled task cannot be started again."
          confirmLabel="Cancel task"
          onConfirm={() => onAction(cancelAction)}
          onClose={() => setAsking(null)}
        />
      )}
    </aside>
  )
}
