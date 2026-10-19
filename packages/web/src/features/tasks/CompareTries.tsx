import type { DiffStat, Task } from '@natamux/core'
import { useState } from 'react'
import { DetailSection } from '../../components/DetailSection'
import { StateBadge } from '../../components/StateBadge'
import { Button } from '../../components/ui/Button'
import { ConfirmDialog } from '../../components/ui/Dialog'
import { pullRequestNumber } from '../../lib/pull-request'

type CompareTriesProps = {
  tries: readonly Task[]
  currentId: string
  diffStats?: Readonly<Record<string, DiffStat>>
  busy: boolean
  onSelect: (id: string) => void
  // Keeps one try and cancels the others that are not finished.
  onKeep: (task: Task) => void
}

// The tries of one task side by side, to pick the one to keep.
export function CompareTries({
  tries,
  currentId,
  diffStats = {},
  busy,
  onSelect,
  onKeep,
}: CompareTriesProps) {
  const [keeping, setKeeping] = useState<Task | null>(null)

  return (
    <DetailSection id="tries" title="Tries" count={tries.length}>
      <table className="w-full text-left">
        <tbody>
          {tries.map((task) => {
            const stat = diffStats[task.id]
            const finished = task.state === 'done' || task.state === 'cancelled'
            const idClass = task.id === currentId ? 'font-semibold' : 'text-link'
            return (
              <tr key={task.id} className="border-b border-line last:border-b-0">
                <td className="py-1 pr-2">
                  <button
                    type="button"
                    onClick={() => onSelect(task.id)}
                    className={`font-mono text-xs ${idClass}`}
                  >
                    {task.id}
                  </button>
                </td>
                <td className="py-1 pr-2">
                  <StateBadge state={task.state} />
                </td>
                <td className="py-1 pr-2 font-mono text-xs text-muted">
                  {stat === undefined || stat.files === 0
                    ? '-'
                    : `+${stat.added} −${stat.removed}, ${stat.files} files`}
                </td>
                <td className="py-1 pr-2 text-xs text-muted">
                  {task.pr === null ? '' : pullRequestNumber(task.pr)}
                </td>
                <td className="py-1 text-right">
                  {!finished && (
                    <Button size="xs" disabled={busy} onClick={() => setKeeping(task)}>
                      Keep this try
                    </Button>
                  )}
                </td>
              </tr>
            )
          })}
        </tbody>
      </table>
      {keeping !== null && (
        <ConfirmDialog
          title={`Keep ${keeping.id}?`}
          description="The other tries that are not finished are cancelled. Their workspaces and branches are kept."
          confirmLabel="Keep this try, cancel the others"
          onConfirm={() => onKeep(keeping)}
          onClose={() => setKeeping(null)}
        />
      )}
    </DetailSection>
  )
}
