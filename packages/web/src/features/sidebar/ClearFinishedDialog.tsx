import type { Task } from '@natamux/core'
import { ConfirmDialog } from '../../components/ui/Dialog'

type ClearFinishedDialogProps = {
  repoName: string
  tasks: readonly Task[]
  onConfirm: (ids: string[]) => void
  onClose: () => void
}

export function ClearFinishedDialog({
  repoName,
  tasks,
  onConfirm,
  onClose,
}: ClearFinishedDialogProps) {
  const count = tasks.length === 1 ? '1 finished workspace' : `${tasks.length} finished workspaces`
  return (
    <ConfirmDialog
      title={`Remove ${count}?`}
      description={`This deletes the worktree folders of these tasks in ${repoName}. A branch stays when it has commits that are not merged. The tasks stay too. A workspace with changes that are not committed is skipped.`}
      confirmLabel="Remove"
      onConfirm={() => onConfirm(tasks.map((task) => task.id))}
      onClose={onClose}
    >
      <ul className="flex max-h-40 flex-col gap-0.5 overflow-y-auto text-xs">
        {tasks.map((task) => (
          <li key={task.id} className="flex min-w-0 gap-2">
            <span className="shrink-0 font-mono text-muted">{task.id}</span>
            <span className="min-w-0 truncate">{task.title}</span>
          </li>
        ))}
      </ul>
    </ConfirmDialog>
  )
}
