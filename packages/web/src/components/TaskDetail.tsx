import { type DiffStat, portRange, type Task } from '@helmsman/core'
import {
  ArrowLeft,
  Ban,
  Copy,
  CopyPlus,
  FileDiff,
  FolderMinus,
  Pencil,
  Play,
  RotateCcw,
  Save,
  X,
} from 'lucide-react'
import { type ReactNode, useState } from 'react'
import type { StoredEvent, TaskChanges } from '../lib/api'
import { copyText } from '../lib/clipboard'
import type { DetailsMode } from '../lib/layout-mode'
import { CHECKS_COLOR, pullRequestSummary } from '../lib/pull-request'
import { actionsFor, agentAction, formatAge, isQuiet, type TaskAction } from '../lib/tasks'
import { CompareTries } from './CompareTries'
import { FanOutDialog } from './FanOutForm'
import { PathText } from './PathText'
import { StateBadge } from './StateBadge'
import { Subtasks } from './Subtasks'
import { TaskEditForm } from './TaskEditForm'
import { Timeline } from './Timeline'
import { Button } from './ui/Button'
import { ConfirmDialog } from './ui/Dialog'
import { IconButton } from './ui/IconButton'
import { Menu } from './ui/Menu'
import { Select } from './ui/Select'
import { button, sectionTitle } from './ui/styles'

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

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section aria-label={title} className="flex flex-col gap-2 border-t border-line pt-4">
      <h3 className={sectionTitle}>{title}</h3>
      {children}
    </section>
  )
}

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="grid grid-cols-[6.5rem_minmax(0,1fr)] items-baseline gap-2 py-0.5">
      <dt className="text-muted">{label}</dt>
      <dd className="min-w-0 break-words">{children}</dd>
    </div>
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

  return (
    <aside
      aria-label={`Details of ${task.id}`}
      className="flex h-full w-full min-w-0 flex-col overflow-hidden bg-surface"
    >
      <header
        className={`flex h-11 shrink-0 items-center gap-1 border-b border-line pr-2 ${mode === 'page' ? 'pl-2' : 'pl-4'}`}
      >
        {mode === 'page' && <IconButton label="Back" icon={ArrowLeft} onClick={onClose} />}
        <span className="flex-1 font-mono text-xs text-muted">{task.id}</span>
        {!editing && (
          <Button variant="ghost" icon={Pencil} onClick={() => setEditing(true)}>
            Edit
          </Button>
        )}
        <Menu
          label={`More actions for ${task.id}`}
          items={[
            ...(onSaveTemplate === undefined
              ? []
              : [{ label: 'Save as template', icon: Save, onSelect: onSaveTemplate }]),
            ...(canFanOut
              ? [{ label: 'Run more tries...', icon: CopyPlus, onSelect: () => setAsking('tries') }]
              : []),
            ...(cancelAction === undefined
              ? []
              : [
                  {
                    label: cancelAction.label,
                    icon: Ban,
                    danger: true,
                    onSelect: () => setAsking('cancel'),
                  },
                ]),
          ]}
        />
        {mode !== 'page' && <IconButton label="Close details" icon={X} onClick={onClose} />}
      </header>

      <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto px-4 py-4">
        <div className="flex flex-col gap-2">
          <h2 className="text-base leading-snug font-semibold tracking-tight break-words">
            {task.title}
          </h2>
          <div className="flex flex-wrap items-center gap-1.5">
            <StateBadge state={task.state} />
            {task.priority !== 'normal' && (
              <span className="rounded-full border border-line px-2 py-px text-xs text-muted capitalize">
                {task.priority}
              </span>
            )}
            {task.labels.map((label) => (
              <span key={label} className="rounded-full bg-hover px-2 py-px text-xs text-muted">
                {label}
              </span>
            ))}
          </div>
        </div>

        {editing && (
          <TaskEditForm
            task={task}
            busy={busy}
            onSave={(changes) => {
              setEditing(false)
              onEdit(changes)
            }}
            onCancel={() => setEditing(false)}
          />
        )}

        {isQuiet(task, now) && (
          <p role="status" className="rounded-md bg-signal-soft px-3 py-2 text-ink">
            No sign of life for {formatAge(task.updatedAt, now).replace(' ago', '')}. The agent may
            be stuck, interrupted or closed.
          </p>
        )}

        {stateActions.length > 0 && (
          <div className="flex flex-wrap items-center gap-2">
            {stateActions.map((action, index) => (
              <button
                key={action.type}
                type="button"
                disabled={busy}
                onClick={() => onAction(action)}
                className={button(index === 0 ? 'primary' : 'secondary')}
              >
                {action.label}
                {action.hotkey ? <kbd className="opacity-60">{action.hotkey}</kbd> : null}
              </button>
            ))}
          </div>
        )}

        {task.description !== '' && (
          <p className="whitespace-pre-wrap text-ink/90">{task.description}</p>
        )}

        {!isFinished && (
          <Section title="Agent">
            <div className="flex flex-wrap items-center gap-2">
              {agentButton !== null && (
                <button
                  type="button"
                  disabled={busy || waitingFor.length > 0}
                  onClick={onStartAgent}
                  data-tip={
                    agentButton === 'restart'
                      ? 'Starts Claude Code again in the terminal of the workspace'
                      : 'Creates the workspace when there is none, then starts Claude Code in its terminal'
                  }
                  className={button(task.workspace === null ? 'primary' : 'secondary')}
                >
                  {agentButton === 'restart' ? (
                    <RotateCcw size={13} aria-hidden="true" />
                  ) : (
                    <Play size={13} aria-hidden="true" />
                  )}
                  {agentButton === 'restart' ? 'Restart agent' : 'Start agent'}
                </button>
              )}
              <span className="text-muted">
                {task.agent === null
                  ? 'No agent yet'
                  : agentButton === null
                    ? `${task.agent} is working here`
                    : task.agent}
              </span>
            </div>
            {waitingFor.length > 0 && (
              <p className="text-muted">Waits for {waitingFor.join(', ')} to be done.</p>
            )}
          </Section>
        )}

        <Section title="Workspace">
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
                <button
                  type="button"
                  disabled={busy}
                  onClick={() => onCreateWorkspace(base === '' ? undefined : base)}
                  className={button()}
                >
                  Create workspace
                </button>
              )}
            </div>
          ) : (
            <>
              <dl>
                <Field label="Branch">
                  {task.branch === null ? '-' : <PathText path={task.branch} />}
                </Field>
                <Field label="Folder">
                  <span className="flex items-start gap-1">
                    <span className="min-w-0 text-xs">
                      <PathText path={task.workspace} />
                    </span>
                    <IconButton
                      label="Copy the folder path"
                      icon={Copy}
                      size="xs"
                      onClick={() => copyText(task.workspace ?? '', 'the folder path')}
                      className="-my-1"
                    />
                  </span>
                </Field>
                <Field label="Ports">
                  <span className="font-mono">
                    {ports.first}–{ports.last}
                  </span>
                  <span className="text-muted"> · PORT is the first</span>
                </Field>
              </dl>
              <div className="flex flex-wrap gap-2">
                <button type="button" onClick={onViewChanges} className={button()}>
                  <FileDiff size={13} aria-hidden="true" />
                  View changes
                </button>
                <button
                  type="button"
                  disabled={busy}
                  onClick={() => setAsking('remove')}
                  className={button('ghost')}
                >
                  <FolderMinus size={13} aria-hidden="true" />
                  Remove workspace
                </button>
              </div>
            </>
          )}
        </Section>

        <Section title="Details">
          <dl>
            {task.repo !== null && <Field label="Repository">{task.repo}</Field>}
            <Field label="Depends on">
              {task.dependsOn.length > 0 ? task.dependsOn.join(', ') : '-'}
            </Field>
            <Field label="Pull request">
              {task.pr === null ? (
                '-'
              ) : (
                <span className="flex flex-wrap items-baseline gap-x-2">
                  <a
                    href={task.pr}
                    target="_blank"
                    rel="noreferrer"
                    className="text-accent hover:underline"
                  >
                    {task.pr.replace(/^https:\/\/github\.com\//, '')}
                  </a>
                  {task.prStatus !== null && (
                    <span className={CHECKS_COLOR[task.prStatus.checks]}>
                      {pullRequestSummary(task)}
                    </span>
                  )}
                  <button
                    type="button"
                    disabled={busy}
                    onClick={onCheckPullRequest}
                    className="text-muted hover:text-ink disabled:opacity-50"
                  >
                    Check now
                  </button>
                </span>
              )}
            </Field>
            {task.parentId !== null && (
              <Field label="Part of">
                <button
                  type="button"
                  onClick={() => onSelectTask?.(task.parentId ?? '')}
                  className="font-mono text-accent hover:underline"
                >
                  {task.parentId}
                </button>
              </Field>
            )}
            <Field label="Created">{formatAge(task.createdAt, now)}</Field>
            <Field label="Updated">{formatAge(task.updatedAt, now)}</Field>
          </dl>
        </Section>

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

        <Timeline taskId={task.id} updatedAt={task.updatedAt} loadEvents={loadEvents} />
      </div>

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
