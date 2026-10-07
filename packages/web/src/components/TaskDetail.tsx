import type { Task } from '@helmsman/core'
import { type ReactNode, useState } from 'react'
import { actionsFor, formatAge, type TaskAction } from '../lib/tasks'
import { StateBadge } from './StateBadge'

type TaskDetailProps = {
  task: Task
  now: Date
  busy: boolean
  feedback: string | null
  onAction: (action: TaskAction) => void
  onCreateWorkspace: () => void
  onRemoveWorkspace: () => void
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
  onAction,
  onCreateWorkspace,
  onRemoveWorkspace,
  onClose,
}: TaskDetailProps) {
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
      className="flex flex-col gap-4 rounded-md border border-line bg-surface p-4"
    >
      <header className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <p className="font-mono text-muted">{task.id}</p>
          <h2 className="text-base font-semibold break-words">{task.title}</h2>
        </div>
        <button
          type="button"
          onClick={onClose}
          className="rounded-md px-2 py-1 text-muted hover:text-ink"
        >
          Close
        </button>
      </header>

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
        <Field label="Agent">{task.agent ?? '-'}</Field>
        <Field label="Branch">
          <span className="font-mono">{task.branch ?? '-'}</span>
        </Field>
        <Field label="Workspace">
          {task.workspace === null ? (
            <span className="flex flex-wrap items-center gap-2">
              <span>-</span>
              {!isFinished && (
                <button
                  type="button"
                  disabled={busy}
                  onClick={onCreateWorkspace}
                  className="rounded-md border border-line px-2 py-1 font-medium hover:border-accent disabled:opacity-50"
                >
                  Create workspace
                </button>
              )}
            </span>
          ) : (
            <span className="flex flex-col items-start gap-2">
              <span className="font-mono break-all">{task.workspace}</span>
              <span className="flex flex-wrap items-center gap-2">
                <button
                  type="button"
                  disabled={busy}
                  onClick={removeWorkspace}
                  className="rounded-md border border-line px-2 py-1 font-medium hover:border-accent disabled:opacity-50"
                >
                  {confirmingRemove ? 'Delete the worktree folder? Click again' : 'Remove workspace'}
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
        <Field label="Labels">{task.labels.length > 0 ? task.labels.join(', ') : '-'}</Field>
        <Field label="Depends on">
          {task.dependsOn.length > 0 ? task.dependsOn.join(', ') : '-'}
        </Field>
        <Field label="Pull request">{task.pr ?? '-'}</Field>
        <Field label="Updated">{formatAge(task.updatedAt, now)}</Field>
      </dl>

      {task.description !== '' && <p className="whitespace-pre-wrap">{task.description}</p>}
    </aside>
  )
}
