import { type DiffStat, portRange, type Task } from '@helmsman/core'
import { type ReactNode, useState } from 'react'
import type { StoredEvent, TaskChanges } from '../lib/api'
import { CHECKS_COLOR, pullRequestSummary } from '../lib/pull-request'
import { actionsFor, formatAge, isQuiet, type TaskAction } from '../lib/tasks'
import { CompareTries } from './CompareTries'
import { FanOutForm } from './FanOutForm'
import { PathText } from './PathText'
import { StateBadge } from './StateBadge'
import { Subtasks } from './Subtasks'
import { TaskEditForm } from './TaskEditForm'
import { Timeline } from './Timeline'

type TaskDetailProps = {
  task: Task
  now: Date
  busy: boolean
  feedback: string | null
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

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex gap-3 py-1">
      <dt className="w-24 shrink-0 text-muted">{label}</dt>
      <dd className="min-w-0 flex-1 break-words">{children}</dd>
    </div>
  )
}

export function TaskDetail({
  task,
  now,
  busy,
  feedback,
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
  const [confirming, setConfirming] = useState(false)
  const [confirmingRemove, setConfirmingRemove] = useState(false)
  const actions = actionsFor(task.state)
  const isFinished = task.state === 'done' || task.state === 'cancelled'

  const removeWorkspace = () => {
    if (!confirmingRemove) {
      setConfirmingRemove(true)
      return
    }
    setConfirmingRemove(false)
    onRemoveWorkspace()
  }

  const runAction = (action: TaskAction) => {
    if (action.confirm && !confirming) {
      setConfirming(true)
      return
    }
    setConfirming(false)
    onAction(action)
  }

  return (
    <aside
      aria-label={`Details of ${task.id}`}
      className="flex w-[22rem] shrink-0 flex-col gap-4 overflow-y-auto border-l border-line bg-surface p-4"
    >
      <header className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <p className="font-mono text-muted">{task.id}</p>
          <h2 className="text-base font-semibold break-words">{task.title}</h2>
        </div>
        <span className="flex shrink-0 gap-1">
          {!editing && (
            <button
              type="button"
              onClick={() => setEditing(true)}
              className="rounded-md px-2 py-1 text-muted hover:text-ink"
            >
              Edit
            </button>
          )}
          {onSaveTemplate !== undefined && (
            <button
              type="button"
              disabled={busy}
              onClick={onSaveTemplate}
              title="Keep the notes, priority and labels to start new tasks from"
              className="rounded-md px-2 py-1 text-muted hover:text-ink disabled:opacity-50"
            >
              Save as template
            </button>
          )}
          <button
            type="button"
            onClick={onClose}
            className="rounded-md px-2 py-1 text-muted hover:text-ink"
          >
            Close
          </button>
        </span>
      </header>

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
        <p role="status" className="border-l-4 border-signal pl-3 font-medium">
          No sign of life for {formatAge(task.updatedAt, now).replace(' ago', '')}. The agent may be
          stuck, interrupted or closed.
        </p>
      )}

      <div className="flex flex-wrap items-center gap-2">
        {actions.map((action) => (
          <button
            key={action.type}
            type="button"
            disabled={busy}
            onClick={() => runAction(action)}
            className="rounded-md border border-line px-3 py-1.5 font-medium hover:border-accent disabled:opacity-50"
          >
            {action.confirm && confirming ? `Cancel ${task.id}? Click again` : action.label}
            {action.hotkey ? <kbd className="ml-2 text-muted">{action.hotkey}</kbd> : null}
          </button>
        ))}
        {confirming && (
          <button type="button" onClick={() => setConfirming(false)} className="px-2 text-muted">
            Keep task
          </button>
        )}
      </div>

      {feedback !== null && (
        <p role="status" className="font-medium">
          {feedback}
        </p>
      )}

      <dl>
        <Field label="State">
          <StateBadge state={task.state} />
        </Field>
        <Field label="Priority">{task.priority}</Field>
        {task.repo !== null && <Field label="Repository">{task.repo}</Field>}
        <Field label="Agent">
          <span className="flex flex-wrap items-center gap-2">
            <span>{task.agent ?? '-'}</span>
            {!isFinished && (
              <button
                type="button"
                disabled={busy || waitingFor.length > 0}
                onClick={onStartAgent}
                title="Creates the workspace when there is none, then starts Claude Code in its terminal"
                className="rounded-md border border-line px-2 py-1 font-medium hover:border-accent disabled:opacity-50"
              >
                Start Claude Code
              </button>
            )}
          </span>
        </Field>
        {!isFinished && onFanOut !== undefined && (
          <Field label="Fan out">
            <FanOutForm busy={busy} onFanOut={onFanOut} />
          </Field>
        )}
        <Field label="Branch">{task.branch === null ? '-' : <PathText path={task.branch} />}</Field>
        <Field label="Workspace">
          {task.workspace === null ? (
            <span className="flex flex-wrap items-center gap-2">
              <span>-</span>
              {!isFinished && branches !== undefined && branches.branches.length > 1 && (
                <select
                  value={base}
                  onChange={(event) => setBase(event.target.value)}
                  aria-label="Start from branch"
                  className="rounded-md border border-line bg-canvas px-1 py-1"
                >
                  <option value="">{branches.current ?? 'HEAD'}</option>
                  {branches.branches
                    .filter((name) => name !== branches.current)
                    .map((name) => (
                      <option key={name} value={name}>
                        {name}
                      </option>
                    ))}
                </select>
              )}
              {!isFinished && (
                <button
                  type="button"
                  disabled={busy}
                  onClick={() => onCreateWorkspace(base === '' ? undefined : base)}
                  className="rounded-md border border-line px-2 py-1 font-medium hover:border-accent disabled:opacity-50"
                >
                  Create workspace
                </button>
              )}
            </span>
          ) : (
            <span className="flex flex-col items-start gap-2">
              <PathText path={task.workspace} />
              <span className="flex flex-wrap items-center gap-2">
                <button
                  type="button"
                  onClick={onViewChanges}
                  className="rounded-md border border-line px-2 py-1 font-medium hover:border-accent"
                >
                  View changes
                </button>
                <button
                  type="button"
                  disabled={busy}
                  onClick={removeWorkspace}
                  className="rounded-md border border-line px-2 py-1 font-medium hover:border-accent disabled:opacity-50"
                >
                  {confirmingRemove
                    ? 'Delete the worktree folder? Click again'
                    : 'Remove workspace'}
                </button>
                {confirmingRemove && (
                  <button
                    type="button"
                    onClick={() => setConfirmingRemove(false)}
                    className="px-2 text-muted"
                  >
                    Keep workspace
                  </button>
                )}
              </span>
            </span>
          )}
        </Field>
        {task.workspace !== null && (
          <Field label="Ports">
            {portRange(task.id).first} to {portRange(task.id).last}
            <span className="block text-muted">PORT in its terminals is the first one.</span>
          </Field>
        )}
        <Field label="Labels">{task.labels.length > 0 ? task.labels.join(', ') : '-'}</Field>
        <Field label="Depends on">
          {task.dependsOn.length > 0 ? task.dependsOn.join(', ') : '-'}
          {waitingFor.length > 0 && (
            <span className="block text-muted">Waits for {waitingFor.join(', ')} to be done.</span>
          )}
        </Field>
        <Field label="Pull request">
          {task.pr === null ? (
            '-'
          ) : (
            <span className="flex flex-wrap items-baseline gap-2">
              <a href={task.pr} target="_blank" rel="noreferrer" className="text-accent underline">
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
                className="rounded-md border border-line px-2 py-0.5 hover:border-accent disabled:opacity-50"
              >
                Check now
              </button>
            </span>
          )}
        </Field>
        <Field label="Updated">{formatAge(task.updatedAt, now)}</Field>
      </dl>

      {task.description !== '' && <p className="whitespace-pre-wrap">{task.description}</p>}

      {task.parentId !== null && (
        <p className="text-muted">
          Part of{' '}
          <button
            type="button"
            onClick={() => onSelectTask?.(task.parentId ?? '')}
            className="font-mono text-accent"
          >
            {task.parentId}
          </button>
        </p>
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

      <Timeline taskId={task.id} updatedAt={task.updatedAt} loadEvents={loadEvents} />
    </aside>
  )
}
