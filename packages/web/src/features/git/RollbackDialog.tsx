import type { ChangeEntry, ChangeStatus } from '@natamux/core'
import { useState } from 'react'
import { FileIcon } from '../../components/FileIcon'
import { Button } from '../../components/ui/Button'
import { Dialog } from '../../components/ui/Dialog'
import { CHANGE_LABEL } from '../../lib/changes'
import { STATUS_TONE } from './ChangedRow'

type RollbackDialogProps = {
  files: readonly ChangeEntry[]
  // Another git action of the task runs: the rollback waits for it.
  gitBusy: boolean
  onConfirm: (paths: string[]) => void
  onClose: () => void
}

// The last commit does not have these files (a rename: its new name), so a rollback deletes them.
const NEW_FILE: ReadonlySet<ChangeStatus> = new Set(['A', 'U', 'R'])

function filesLabel(count: number): string {
  return count === 1 ? '1 file' : `${count} files`
}

// Asks which files go back to the last commit. All are checked at first, except a conflict, which
// has to be resolved instead. Cancel has the focus, so Enter is the safe answer.
export function RollbackDialog({ files, gitBusy, onConfirm, onClose }: RollbackDialogProps) {
  const open = files.filter((entry) => entry.status !== 'C')
  const [checked, setChecked] = useState<ReadonlySet<string>>(
    () => new Set(open.map((entry) => entry.path)),
  )
  const picked = open.filter((entry) => checked.has(entry.path))
  const all = open.length > 0 && picked.length === open.length
  const newFiles = picked.filter((entry) => NEW_FILE.has(entry.status)).length
  const toggle = (path: string, on: boolean) => {
    const next = new Set(checked)
    if (on) {
      next.add(path)
    } else {
      next.delete(path)
    }
    setChecked(next)
  }

  return (
    <Dialog
      title="Roll back changes"
      description="The checked files go back to how they are in the last commit."
      size="md"
      onClose={onClose}
      footer={
        <>
          <Button variant="ghost" data-autofocus onClick={onClose}>
            Cancel
          </Button>
          <Button
            variant="danger"
            disabledReason={
              picked.length === 0
                ? 'Check a file first'
                : gitBusy
                  ? 'Another git action is running'
                  : undefined
            }
            onClick={() => {
              onClose()
              onConfirm(picked.map((entry) => entry.path))
            }}
          >
            Roll back {filesLabel(picked.length)}
          </Button>
        </>
      }
    >
      <label className="flex h-7 items-center gap-2 border-b border-line text-sm font-medium">
        <input
          type="checkbox"
          checked={all}
          ref={(node) => {
            if (node !== null) {
              node.indeterminate = picked.length > 0 && !all
            }
          }}
          disabled={open.length === 0}
          onChange={(event) =>
            setChecked(new Set(event.target.checked ? open.map((entry) => entry.path) : []))
          }
          className="size-3.5 accent-accent"
        />
        All files
      </label>
      <ul aria-label="Files to roll back" className="flex max-h-72 flex-col overflow-y-auto py-1">
        {files.map((entry) => {
          const conflict = entry.status === 'C'
          return (
            <li key={entry.path} data-tip={conflict ? 'Resolve the conflict first' : undefined}>
              <label className="flex h-7 min-w-0 items-center gap-2 text-sm">
                <input
                  type="checkbox"
                  aria-label={`${entry.path}, ${CHANGE_LABEL[entry.status]}`}
                  checked={!conflict && checked.has(entry.path)}
                  disabled={conflict}
                  onChange={(event) => toggle(entry.path, event.target.checked)}
                  className="size-3.5 shrink-0 accent-accent"
                />
                <FileIcon path={entry.path} />
                <span className="min-w-0 flex-1 truncate">{entry.path}</span>
                <span
                  aria-hidden="true"
                  className={`shrink-0 font-mono text-xs font-semibold ${STATUS_TONE[entry.status]}`}
                >
                  {entry.status}
                </span>
              </label>
            </li>
          )
        })}
      </ul>
      {newFiles > 0 && (
        <p className="mt-2 text-danger">
          {newFiles === 1
            ? '1 new file will be deleted.'
            : `${newFiles} new files will be deleted.`}
        </p>
      )}
    </Dialog>
  )
}
