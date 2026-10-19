import type { Task } from '@natamux/core'
import { useQuery } from '@tanstack/react-query'
import { Ban, CircleCheck, ExternalLink, FolderGit2, Play } from 'lucide-react'
import { useEffect, useRef, useState } from 'react'
import { PathText } from '../../components/PathText'
import { StateBadge } from '../../components/StateBadge'
import { Button } from '../../components/ui/Button'
import { Combobox } from '../../components/ui/Combobox'
import { EmptyState } from '../../components/ui/EmptyState'
import { button, ICON } from '../../components/ui/styles'
import type { BranchList, StoredEvent } from '../../lib/api'
import { finishedAt } from '../../lib/finished'
import { pullRequestNumber, pullRequestSummary } from '../../lib/pull-request'
import { formatExact } from '../../lib/tasks'
import { branchOptions, TASK_BRANCH_GROUP } from './branches'

type NoWorkspaceProps = {
  task: Task
  busy: boolean
  // Tasks this one waits for. The agent cannot start before they are done.
  waitingFor: readonly string[]
  // Local branches a workspace can start from, the one the checkout is on, and the default one.
  branches?: BranchList
  // The branch of the parent of a subtask. It is the default base when it is in the list.
  parentBranch?: string | null
  onCreateWorkspace: (base?: string) => void
  onStartAgent: (base?: string) => void
  // For a done or cancelled task: its progress in the details, and a new task that goes on from it.
  onShowProgress?: () => void
  onFollowUp?: () => void
  // The events of a finished task, for the time it finished.
  loadEvents?: () => Promise<StoredEvent[]>
}

// What a done or cancelled task left behind, once its workspace is gone, and where to go next.
function FinishedSummary({
  task,
  onShowProgress,
  onFollowUp,
  loadEvents,
}: Pick<NoWorkspaceProps, 'task' | 'onShowProgress' | 'onFollowUp' | 'loadEvents'>) {
  const heading = useRef<HTMLHeadingElement>(null)
  const done = task.state === 'done'
  const Icon = done ? CircleCheck : Ban
  const summary = pullRequestSummary(task)
  // The same key as the activity in the details.
  const events = useQuery({
    queryKey: ['events', task.id],
    queryFn: () => loadEvents?.() ?? Promise.resolve([]),
    enabled: loadEvents !== undefined,
  })
  const finished = events.data === undefined ? null : finishedAt(events.data)

  // The Remove button that had the focus is gone with the workspace view.
  useEffect(() => {
    if (document.activeElement === null || document.activeElement === document.body) {
      heading.current?.focus()
    }
  }, [])

  return (
    <div className="w-full max-w-130 rounded-xl border border-line bg-surface p-6">
      <Icon size={ICON.xl} aria-hidden="true" className={done ? 'text-add' : 'text-muted'} />
      <h3 ref={heading} tabIndex={-1} className="mt-3 text-lg font-semibold tracking-tight">
        {done ? `${task.id} is done` : `${task.id} was cancelled`}
      </h3>
      <p className="mt-1 text-ink-2">
        Its workspace was removed. The task, its notes, docs and history stay.
      </p>
      <dl className="mt-4 grid grid-cols-[8rem_minmax(0,1fr)] gap-x-3 gap-y-2">
        <dt className="text-muted">Pull request</dt>
        <dd>
          {task.pr === null
            ? 'None'
            : `${pullRequestNumber(task.pr)}${summary === null ? '' : `, ${summary}`}`}
        </dd>
        <dt className="text-muted">Branch</dt>
        <dd className="min-w-0 break-all">
          {task.branch === null ? (
            'Deleted'
          ) : (
            <>
              Kept: <PathText path={task.branch} />
            </>
          )}
        </dd>
        {finished !== null && (
          <>
            <dt className="text-muted">Finished</dt>
            <dd>{formatExact(finished)}</dd>
          </>
        )}
      </dl>
      <div className="mt-5 flex flex-wrap gap-2">
        {task.pr !== null && (
          <a href={task.pr} target="_blank" rel="noreferrer" className={button('secondary', 'md')}>
            Open pull request
            <ExternalLink size={ICON.xs} aria-hidden="true" />
          </a>
        )}
        {onShowProgress !== undefined && (
          <Button size="md" onClick={onShowProgress}>
            Show progress
          </Button>
        )}
        {onFollowUp !== undefined && (
          <Button size="md" onClick={onFollowUp}>
            Start a follow-up task
          </Button>
        )}
      </div>
    </div>
  )
}

// What the main area shows for a task that has no worktree yet.
export function NoWorkspace({
  task,
  busy,
  waitingFor,
  branches,
  parentBranch = null,
  onCreateWorkspace,
  onStartAgent,
  onShowProgress,
  onFollowUp,
  loadEvents,
}: NoWorkspaceProps) {
  const [base, setBase] = useState<string | undefined>(undefined)
  const finished = task.state === 'done' || task.state === 'cancelled'
  const start =
    parentBranch !== null && branches?.branches.includes(parentBranch)
      ? parentBranch
      : (branches?.defaultBranch ?? branches?.current ?? null)
  const from = base ?? start ?? 'HEAD'
  const blocked =
    waitingFor.length > 0 ? `Waits for ${waitingFor.join(', ')} to be done` : undefined

  const header = (
    <header className="flex min-h-11 shrink-0 items-center gap-3 border-b border-line px-4 py-1.5">
      <span className="shrink-0 font-mono text-xs text-muted">{task.id}</span>
      <h2 data-tip-overflow className="min-w-0 truncate text-lg font-semibold tracking-tight">
        {task.title}
      </h2>
      <StateBadge state={task.state} />
    </header>
  )

  if (finished) {
    return (
      <section aria-label={`Workspace of ${task.id}`} className="flex min-h-0 flex-1 flex-col">
        {header}
        <div className="flex min-h-0 flex-1 items-center justify-center overflow-y-auto p-4">
          <FinishedSummary
            task={task}
            onShowProgress={onShowProgress}
            onFollowUp={onFollowUp}
            loadEvents={loadEvents}
          />
        </div>
      </section>
    )
  }

  return (
    <section aria-label={`Workspace of ${task.id}`} className="flex min-h-0 flex-1 flex-col">
      {header}
      <div className="flex min-h-0 flex-1 items-center justify-center overflow-y-auto">
        <EmptyState icon={FolderGit2} title="No workspace">
          {`${task.id} has no workspace yet. Create one to work on it, or start an agent in a new one.`}
          <span className="mt-4 flex flex-col items-center gap-3">
            {branches !== undefined && branches.branches.length > 1 && (
              <span className="flex w-full max-w-72 flex-col gap-1 text-left">
                <span className="text-xs font-medium text-ink-2">Branch from</span>
                <Combobox
                  label="Branch from"
                  placeholder={from}
                  options={branchOptions(branches.branches, branches.current, null, start)}
                  value={base ?? start ?? ''}
                  onChange={(name) => setBase(name === start ? undefined : name)}
                  collapsedGroups={[TASK_BRANCH_GROUP]}
                />
              </span>
            )}
            <span className="flex items-center gap-2">
              <Button
                variant="secondary"
                size="md"
                disabled={busy}
                onClick={() => onCreateWorkspace(base)}
              >
                Create workspace
              </Button>
              <Button
                variant="primary"
                size="md"
                icon={Play}
                disabled={busy}
                disabledReason={blocked}
                onClick={() => onStartAgent(base)}
              >
                Start agent
              </Button>
            </span>
          </span>
        </EmptyState>
      </div>
    </section>
  )
}
