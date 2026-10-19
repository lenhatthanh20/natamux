import type { Task } from '@natamux/core'
import { type FormEvent, useState } from 'react'
import { Button } from '../../components/ui/Button'
import { Dialog } from '../../components/ui/Dialog'
import { inputLg } from '../../components/ui/styles'

type AddSubtaskDialogProps = {
  parent: Task
  busy: boolean
  onAdd: (title: string) => void
  onClose: () => void
}

// Asks for the title of a subtask from the row menu. The caller closes it on success, so a failed
// request leaves the title here.
export function AddSubtaskDialog({ parent, busy, onAdd, onClose }: AddSubtaskDialogProps) {
  const [title, setTitle] = useState('')
  const empty = title.trim() === ''

  const submit = (event?: FormEvent) => {
    event?.preventDefault()
    if (!empty && !busy) {
      onAdd(title.trim())
    }
  }

  return (
    <Dialog
      title={`Add a subtask to ${parent.id}`}
      size="sm"
      busy={busy}
      onClose={onClose}
      onSubmit={submit}
      footer={
        <>
          <Button variant="ghost" size="md" onClick={onClose}>
            Cancel
          </Button>
          <Button
            type="submit"
            form="add-subtask"
            variant="primary"
            size="md"
            shortcut="Mod+Enter"
            busy={busy}
            disabled={empty || busy}
          >
            Add subtask
          </Button>
        </>
      }
    >
      <form id="add-subtask" onSubmit={submit}>
        <label className="flex flex-col gap-1">
          <span className="text-xs font-medium text-muted">Title</span>
          <input
            value={title}
            onChange={(event) => setTitle(event.target.value)}
            data-autofocus
            className={inputLg}
          />
        </label>
      </form>
    </Dialog>
  )
}
