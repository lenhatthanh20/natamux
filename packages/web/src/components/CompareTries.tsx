import type { DiffStat, Task } from '@helmsman/core'
import { useState } from 'react'
import { pullRequestNumber } from '../lib/pull-request'
import { StateBadge } from './StateBadge'
import { ConfirmDialog } from './ui/Dialog'
import { button, sectionTitle } from './ui/styles'

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
    <section aria-label="Tries" className="flex flex-col gap-2 border-t border-line pt-4">
      <h3 className={sectionTitle}>Tries</h3>
      <table className="w-full text-left">
        <tbody>
          {tries.map((task) => {
            const stat = diffStats[task.id]
            const finished = task.state === 'done' || task.state === 'cancelled'
            const idClass = task.id === currentId ? 'font-semibold' : 'text-accent'
            return (
              <tr key={task.id} className="border-b border-line last:border-b-0">
                <td className="py-1 pr-2">
                  <button
                    type="button"
                    onClick={() => onSelect(task.id)}
                    className={`font-mono ${idClass}`}
                  >
                    {task.id}
                  </button>
                </td>
                <td className="py-1 pr-2">
                  <StateBadge state={task.state} />
                </td>
                <td className="py-1 pr-2 font-mono text-muted">
                  {stat === undefined || stat.files === 0
                    ? '-'
                    : `+${stat.added} −${stat.removed}, ${stat.files} files`}
                </td>
                <td className="py-1 pr-2 text-muted">
                  {task.pr === null ? '' : pullRequestNumber(task.pr)}
                </td>
                <td className="py-1 text-right">
                  {!finished && (
                    <button
                      type="button"
                      disabled={busy}
                      onClick={() => setKeeping(task)}
                      className={button()}
                    >
                      Keep this one
                    </button>
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
          description="The other tries that are not finished are cancelled."
          confirmLabel="Cancel the others"
          onConfirm={() => onKeep(keeping)}
          onClose={() => setKeeping(null)}
        />
      )}
    </section>
  )
}
