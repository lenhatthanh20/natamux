import { type DiffStat, portRange, type Task, type TaskUsage } from '@natamux/core'
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
  Trash2,
  X,
} from 'lucide-react'
import { type ReactNode, useState } from 'react'
import { Age } from '../../components/Age'
import { DetailSection, Field } from '../../components/DetailSection'
import { PathText } from '../../components/PathText'
import { StateBadge } from '../../components/StateBadge'
import { Button } from '../../components/ui/Button'
import { Combobox } from '../../components/ui/Combobox'
import { ConfirmDialog } from '../../components/ui/Dialog'
import { IconButton } from '../../components/ui/IconButton'
import { Menu, type MenuItem } from '../../components/ui/Menu'
import { ICON } from '../../components/ui/styles'
import { Tabs, tabPanelProps } from '../../components/ui/Tabs'
import { useNowValue } from '../../hooks/useNow'
import type { BranchList, StoredEvent, TaskChanges } from '../../lib/api'
import { useAppEvent } from '../../lib/app-events'
import { copyText } from '../../lib/clipboard'
import { shortcutOf } from '../../lib/commands'
import type { DetailsMode } from '../../lib/layout-mode'
import { CHECKS_COLOR, pullRequestSummary } from '../../lib/pull-request'
import {
  actionsFor,
  agentAction,
  cancelTaskDescription,
  deleteTaskDescription,
  formatExact,
  isQuiet,
  type TaskAction,
} from '../../lib/tasks'
import type { DetailsView } from '../../lib/view-store/helpers'
import { branchOptions, TASK_BRANCH_GROUP } from '../workspaces/branches'
import { CompareTries } from './CompareTries'
import { FanOutDialog } from './FanOutForm'
import { Subtasks } from './Subtasks'
import { LABEL_CHIP } from './TaskChips'
import { TaskEditForm } from './TaskEditForm'
import { Timeline } from './Timeline'
import { UsageSection } from './UsageSection'

type TaskDetailProps = {
  task: Task
  busy: boolean
  // Docked beside the main area, a sheet over it, or a page that replaces it. The parent sizes it.
  mode?: DetailsMode
  loadEvents: () => Promise<StoredEvent[]>
  onAction: (action: TaskAction) => void
  onCreateWorkspace: (base?: string) => void
  // Local branches a workspace can start from, the one the checkout is on, and the default one.
  branches?: BranchList
  // The branch of the parent of a subtask. It is the default base when it is in the list.
  parentBranch?: string | null
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
  // The tokens and estimated cost of the task. Undefined: none yet.
  usage?: TaskUsage
  onKeepTry?: (task: Task) => void
  onDelete?: () => void
  onClose: () => void
  // The progress of the task. When it is set, a row of tabs switches between it and the details.
  progress?: ReactNode
  // The files of the worktree, for a task that has one. A Files tab sits between the two above.
  files?: ReactNode
  view?: DetailsView
  onViewChange?: (view: DetailsView) => void
}

// "tries-t-0002" is how the daemon groups tries. People read it as "Tries of T-0002". A chosen id
// works the same: "tries-task-0005" is "Tries of TASK-0005".
export function labelText(label: string): string {
  const match = /^tries-([a-z]+-\d+)$/i.exec(label)
  return match?.[1] === undefined ? label : `Tries of ${match[1].toUpperCase()}`
}

export function TaskDetail({
  task,
  busy,
  mode = 'docked',
  loadEvents,
  onAction,
  onCreateWorkspace,
  branches,
  parentBranch = null,
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
  usage,
  onKeepTry,
  onDelete,
  onClose,
  progress,
  files,
  view = 'details',
  onViewChange,
}: TaskDetailProps) {
  const [editing, setEditing] = useState(false)
  const [base, setBase] = useState('')
  const start =
    parentBranch !== null && branches?.branches.includes(parentBranch)
      ? parentBranch
      : (branches?.defaultBranch ?? branches?.current ?? null)
  const [asking, setAsking] = useState<'cancel' | 'delete' | 'tries' | null>(null)
  const actions = actionsFor(task.state)
  const stateActions = actions.filter((action) => !action.confirm)
  const cancelAction = actions.find((action) => action.confirm)
  const isFinished = task.state === 'done' || task.state === 'cancelled'
  const ports = portRange(task.id)
  const agentButton = agentAction(task)
  const quiet = useNowValue((now) => isQuiet(task, now))
  // Copies of a task that works now would run beside its own agent.
  const canFanOut = onFanOut !== undefined && !isFinished && agentButton !== null
  const page = mode === 'page'
  // The edit form is part of the details, so it shows them while it is open.
  // Files picked last falls back to the progress for a task without a worktree.
  const picked = view === 'files' && files === undefined ? 'progress' : view
  const shown: DetailsView = progress === undefined || editing ? 'details' : picked

  // The E key is a command, see lib/commands/.
  useAppEvent('edit-task', () => setEditing(true))

  const leading: MenuItem[] = [
    ...(onSaveTemplate === undefined
      ? []
      : [{ label: 'Save as template', icon: Save, onSelect: onSaveTemplate }]),
    ...(canFanOut
      ? [{ label: 'Run more tries...', icon: CopyPlus, onSelect: () => setAsking('tries') }]
      : []),
  ]
  const danger: MenuItem[] = [
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
    ...(onDelete === undefined
      ? []
      : [
          {
            label: 'Delete task',
            icon: Trash2,
            danger: true,
            onSelect: () => setAsking('delete'),
          },
        ]),
  ]
  const menu: MenuItem[] = [
    ...leading,
    ...(leading.length > 0 && danger.length > 0 ? [{ type: 'separator' as const }] : []),
    ...danger,
  ]

  return (
    <aside
      aria-label={`Details of ${task.id}`}
      className="flex h-full w-full min-w-0 flex-col overflow-hidden bg-surface"
    >
      <header
        className={`flex h-10 shrink-0 items-center gap-1 border-b border-line pr-2 ${page ? 'pl-2' : 'pl-4'}`}
      >
        {page && <IconButton label="Back" icon={ArrowLeft} onClick={onClose} />}
        <span className="flex-1 font-mono text-xs text-muted">{task.id}</span>
        {!editing && (
          <IconButton
            label="Edit"
            icon={Pencil}
            shortcut={shortcutOf('task.edit')}
            onClick={() => setEditing(true)}
          />
        )}
        {menu.length > 0 && <Menu label={`More actions for ${task.id}`} items={menu} size="sm" />}
        {!page && <IconButton label="Close details" icon={X} shortcut="Esc" onClick={onClose} />}
      </header>

      {progress !== undefined && (
        <Tabs
          label="View of the details"
          idPrefix="details"
          value={shown}
          onChange={(next) => {
            // Leaving the form would drop its unsaved changes; Save or Cancel closes it.
            if (next !== shown && !editing) {
              onViewChange?.(next)
            }
          }}
          className="px-2"
          background="surface"
          tabs={[
            { id: 'progress', label: 'Progress' },
            ...(files === undefined ? [] : [{ id: 'files' as const, label: 'Files' }]),
            { id: 'details', label: 'Details' },
          ]}
        />
      )}

      {shown === 'progress' ? (
        <div
          {...tabPanelProps('details', 'progress', 'Progress')}
          className="flex min-h-0 flex-1 flex-col"
        >
          {progress}
        </div>
      ) : shown === 'files' ? (
        <div
          {...tabPanelProps('details', 'files', 'Files')}
          className="flex min-h-0 flex-1 flex-col"
        >
          {files}
        </div>
      ) : editing ? (
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
        <div
          {...(progress === undefined ? {} : tabPanelProps('details', 'details', 'Details'))}
          className="flex min-h-0 flex-1 flex-col overflow-y-auto"
        >
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

          {(quiet || waitingFor.length > 0) && (
            <div className="flex flex-col gap-2 px-4 pb-4">
              {quiet && (
                <p
                  role="status"
                  className="rounded-md border border-signal-line bg-signal-soft px-3 py-2.5 text-ink"
                >
                  No sign of life for <Age at={task.updatedAt} />. The agent may be stuck.
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
              {page && (
                <span className="text-2xs font-semibold tracking-[0.06em] text-muted uppercase">
                  Actions
                </span>
              )}
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
                  <Combobox
                    label="Start from branch"
                    placeholder={start ?? 'HEAD'}
                    options={branchOptions(branches.branches, branches.current, null, start)}
                    value={base === '' ? (start ?? '') : base}
                    onChange={(name) => setBase(name === start ? '' : name)}
                    collapsedGroups={[TASK_BRANCH_GROUP]}
                    className="w-48"
                  />
                )}
                {isFinished ? (
                  <span className="text-muted">No workspace.</span>
                ) : (
                  <Button
                    disabled={busy}
                    onClick={() => onCreateWorkspace(base === '' ? undefined : base)}
                  >
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
                    {task.branch === null ? (
                      '-'
                    ) : (
                      <span className="break-all">
                        <PathText path={task.branch} />
                      </span>
                    )}
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
                    <span className="text-xs break-all">
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
                  <Button
                    variant="ghost"
                    icon={FolderMinus}
                    disabled={busy}
                    onClick={onRemoveWorkspace}
                  >
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

          <UsageSection task={task} usage={usage} />

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
              {task.repo !== null && <Field label="Project">{task.repo}</Field>}
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
                  <Age at={task.createdAt} style="long" />
                </span>
              </Field>
              <Field label="Updated">
                <span data-tip={formatExact(task.updatedAt)}>
                  <Age at={task.updatedAt} style="long" />
                </span>
              </Field>
            </dl>
          </DetailSection>

          <Timeline taskId={task.id} loadEvents={loadEvents} />
        </div>
      )}

      {asking === 'tries' && onFanOut !== undefined && (
        <FanOutDialog busy={busy} onFanOut={onFanOut} onClose={() => setAsking(null)} />
      )}
      {asking === 'cancel' && cancelAction !== undefined && (
        <ConfirmDialog
          title={`Cancel ${task.id}?`}
          description={cancelTaskDescription(task)}
          confirmLabel="Cancel task"
          cancelLabel="Keep working"
          onConfirm={() => onAction(cancelAction)}
          onClose={() => setAsking(null)}
        />
      )}
      {asking === 'delete' && onDelete !== undefined && (
        <ConfirmDialog
          title={`Delete ${task.id}?`}
          description={deleteTaskDescription(task)}
          confirmLabel="Delete task"
          onConfirm={onDelete}
          onClose={() => setAsking(null)}
        />
      )}
    </aside>
  )
}
