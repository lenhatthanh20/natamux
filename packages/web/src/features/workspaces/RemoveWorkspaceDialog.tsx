import type { ChangeEntry, GitGraph, Task } from '@natamux/core'
import { useQuery } from '@tanstack/react-query'
import { Check, type LucideIcon, Minus, TriangleAlert } from 'lucide-react'
import { type ReactNode, useState } from 'react'
import { PathText } from '../../components/PathText'
import { Button } from '../../components/ui/Button'
import { Dialog } from '../../components/ui/Dialog'
import { ICON } from '../../components/ui/styles'
import type { TaskShells } from '../../lib/api'
import { CHANGE_LABEL } from '../../lib/changes'
import { branchChoice, busyShells, isAgentShell, isFinishedState } from '../../lib/finished'
import { taskShellsQuery } from '../../lib/task-shells'
import { STATE_LABEL } from '../../lib/tasks'

type RemoveWorkspaceDialogProps = {
  task: Task
  loadChanges: () => Promise<ChangeEntry[]>
  loadGraph: () => Promise<GitGraph>
  loadShells: (taskId: string) => Promise<TaskShells>
  // Resolves once the workspace is gone. A rejection is shown in the dialog, which stays open.
  remove: (deleteBranch: boolean) => Promise<unknown>
  onClose: () => void
  onViewChanges: () => void
  onCommit: () => void
}

// Files listed in the dialog that asks for a commit first.
const SHOWN_FILES = 5
const NOT_COMMITTED =
  'The workspace cannot be removed while it has changes. Commit them, or discard them in Changes.'

const TONE = { remove: 'text-remove', warn: 'text-signal', keep: 'text-add' } as const

function Row({
  icon: Icon,
  tone,
  children,
}: {
  icon: LucideIcon
  tone: keyof typeof TONE
  children: ReactNode
}) {
  return (
    <li className="flex gap-2.5">
      <Icon size={ICON.sm} aria-hidden="true" className={`mt-0.5 shrink-0 ${TONE[tone]}`} />
      <div className="min-w-0">{children}</div>
    </li>
  )
}

function shellCount(count: number, other: boolean): string {
  const word = other ? ' other' : ''
  return count === 1 ? `1${word} shell` : `${count}${word} shells`
}

// Asks before the worktree of a task is deleted. It says what goes and what stays, refuses a
// worktree with changes that are not committed, and stays open until the daemon answers.
export function RemoveWorkspaceDialog({
  task,
  loadChanges,
  loadGraph,
  loadShells,
  remove,
  onClose,
  onViewChanges,
  onCommit,
}: RemoveWorkspaceDialogProps) {
  // The same keys as the workspace view, so an open workspace answers at once.
  const changes = useQuery({ queryKey: ['changes', task.id], queryFn: loadChanges })
  const graph = useQuery({ queryKey: ['graph', task.id], queryFn: loadGraph })
  const shells = useQuery(taskShellsQuery(task.id, loadShells))
  const [keepBranch, setKeepBranch] = useState(false)
  const [removing, setRemoving] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const files = changes.data ?? []
  if (files.length > 0) {
    const more = files.length - SHOWN_FILES
    const title =
      files.length === 1 ? '1 file is not committed' : `${files.length} files are not committed`
    return (
      <Dialog
        key="changes"
        title={title}
        description={NOT_COMMITTED}
        onClose={onClose}
        footer={
          <>
            <Button variant="ghost" data-autofocus onClick={onClose}>
              Close
            </Button>
            <Button onClick={onViewChanges}>View changes</Button>
            <Button variant="primary" onClick={onCommit}>
              Commit…
            </Button>
          </>
        }
      >
        <ul className="flex flex-col gap-1 rounded-md bg-canvas px-3 py-2 font-mono text-xs">
          {files.slice(0, SHOWN_FILES).map((file) => (
            <li key={file.path} className="flex gap-2">
              <span data-tip={CHANGE_LABEL[file.status]} className="shrink-0 text-muted">
                {file.status}
              </span>
              <span className="min-w-0 truncate">{file.path}</span>
            </li>
          ))}
        </ul>
        {more > 0 && <p className="mt-2 text-xs text-muted">and {more} more</p>}
      </Dialog>
    )
  }

  const choice = branchChoice(task, graph.data ?? null)
  const deleteBranch = choice.canDelete && !keepBranch
  const busy = shells.data === undefined ? [] : busyShells(shells.data)
  // An agent shell has its own row, "Stops the agent".
  const data = shells.data
  const others =
    data === undefined ? [] : data.terminals.filter((name) => !isAgentShell(data, name))
  const plain = others.length - busy.length
  const shellsText =
    busy.length > 0
      ? `Ends ${shellCount(plain, true)}.`
      : `Ends ${shellCount(plain, false)}. Nothing runs in them.`
  let confirmLabel = busy.length > 0 ? 'Stop it and remove' : 'Remove workspace'
  if (removing) {
    confirmLabel = 'Removing…'
  }

  const confirm = async () => {
    setRemoving(true)
    setError(null)
    try {
      await remove(deleteBranch)
      onClose()
    } catch (failure) {
      setError(failure instanceof Error ? failure.message : String(failure))
      setRemoving(false)
    }
  }

  return (
    <Dialog
      key="remove"
      title={`Remove the workspace of ${task.id}?`}
      busy={removing}
      onClose={() => {
        if (!removing) {
          onClose()
        }
      }}
      footer={
        <>
          <Button variant="ghost" data-autofocus disabled={removing} onClick={onClose}>
            Cancel
          </Button>
          <Button
            variant="danger"
            busy={removing}
            disabledReason={changes.isPending ? 'Reading the changes…' : undefined}
            onClick={() => void confirm()}
          >
            {confirmLabel}
          </Button>
        </>
      }
    >
      <ul className="flex flex-col gap-3">
        <Row icon={Minus} tone="remove">
          Deletes the folder
          <div className="text-xs break-all text-muted">
            <PathText path={task.workspace ?? ''} />
          </div>
        </Row>
        {!isFinishedState(task.state) && (
          <Row icon={TriangleAlert} tone="warn">
            {`Stops the agent. ${task.id} stays ${STATE_LABEL[task.state].toLowerCase()}.`}
          </Row>
        )}
        {busy.map(({ name, program }) => (
          <Row key={name} icon={TriangleAlert} tone="warn">
            Stops {program} in Shell {name}
          </Row>
        ))}
        {plain > 0 && (
          <Row icon={Minus} tone="remove">
            {shellsText}
          </Row>
        )}
        {task.branch !== null && (
          <li>
            <label className="flex gap-2.5">
              <input
                type="checkbox"
                checked={deleteBranch}
                disabled={!choice.canDelete || removing}
                onChange={(event) => setKeepBranch(!event.target.checked)}
                className="mt-0.5 size-3.5 shrink-0 accent-accent"
              />
              <span className={choice.canDelete ? '' : 'text-muted'}>
                Also delete the branch
                <span className="block text-xs text-muted">{choice.note}</span>
              </span>
            </label>
          </li>
        )}
        <Row icon={Check} tone="keep">
          Keeps the task, its notes, docs and history.
        </Row>
      </ul>
      {error !== null && (
        <p
          role="alert"
          className="mt-3 rounded-md border border-line bg-canvas px-3 py-2 text-xs text-remove"
        >
          {error}
        </p>
      )}
    </Dialog>
  )
}
