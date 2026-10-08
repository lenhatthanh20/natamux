import { TASK_PRIORITIES, type Task } from '@helmsman/core'
import { type FormEvent, useEffect, useRef, useState } from 'react'
import type { TaskChanges } from '../lib/api'
import { Button } from './ui/Button'
import { Select } from './ui/Select'
import { input, textarea } from './ui/styles'

type TaskEditFormProps = {
  task: Task
  busy: boolean
  onSave: (changes: TaskChanges) => void
  onCancel: () => void
}

// "ui, parser , ,api" becomes ["ui", "parser", "api"].
export function parseLabels(text: string): string[] {
  return [
    ...new Set(
      text
        .split(',')
        .map((label) => label.trim())
        .filter((label) => label !== ''),
    ),
  ]
}

// "t-1, T-0002 x" becomes ["T-1", "T-0002"]: anything that is not a task id is dropped.
export function parseTaskIds(text: string): string[] {
  return [
    ...new Set(
      text
        .split(/[\s,]+/)
        .map((id) => id.trim().toUpperCase())
        .filter((id) => /^T-\d+$/.test(id)),
    ),
  ]
}

const inputClass = `${input} w-full`

// The form that takes the place of the title and the notes. The title has the focus, and the
// buttons stay at the bottom of the panel while the fields scroll.
export function TaskEditForm({ task, busy, onSave, onCancel }: TaskEditFormProps) {
  const [title, setTitle] = useState(task.title)
  const [description, setDescription] = useState(task.description)
  const [priority, setPriority] = useState(task.priority)
  const [labels, setLabels] = useState(task.labels.join(', '))
  const [dependsOn, setDependsOn] = useState(task.dependsOn.join(', '))
  const titleField = useRef<HTMLInputElement>(null)

  useEffect(() => {
    titleField.current?.select()
  }, [])

  const save = () => {
    if (title.trim() === '' || busy) {
      return
    }
    onSave({
      title: title.trim(),
      description,
      priority,
      labels: parseLabels(labels),
      dependsOn: parseTaskIds(dependsOn),
    })
  }

  const submit = (event: FormEvent) => {
    event.preventDefault()
    save()
  }

  return (
    <form
      onSubmit={submit}
      onKeyDown={(event) => {
        if (event.key === 'Enter' && (event.metaKey || event.ctrlKey)) {
          event.preventDefault()
          save()
        }
      }}
      aria-label={`Edit ${task.id}`}
      className="flex min-h-0 flex-1 flex-col"
    >
      <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto p-4">
        <label className="flex flex-col gap-1">
          <span className="text-xs font-medium text-ink-2">Title</span>
          <input
            ref={titleField}
            value={title}
            onChange={(event) => setTitle(event.target.value)}
            className={inputClass}
          />
        </label>
        <label className="flex flex-col gap-1">
          <span className="text-xs font-medium text-ink-2">Notes (markdown)</span>
          <textarea
            rows={8}
            value={description}
            onChange={(event) => setDescription(event.target.value)}
            className={`${textarea} w-full font-mono text-xs`}
          />
        </label>
        <div className="flex gap-3">
          <label className="flex flex-1 flex-col gap-1" htmlFor="edit-priority">
            <span className="text-xs font-medium text-ink-2">Priority</span>
            <Select
              id="edit-priority"
              value={priority}
              onChange={(event) => setPriority(event.target.value as Task['priority'])}
              className="w-full"
            >
              {TASK_PRIORITIES.map((value) => (
                <option key={value} value={value}>
                  {value}
                </option>
              ))}
            </Select>
          </label>
          <label className="flex flex-[2] flex-col gap-1">
            <span className="text-xs font-medium text-ink-2">Labels, comma separated</span>
            <input
              value={labels}
              onChange={(event) => setLabels(event.target.value)}
              className={inputClass}
            />
          </label>
        </div>
        <label className="flex flex-col gap-1">
          <span className="text-xs font-medium text-ink-2">
            Waits for tasks (ids, comma separated)
          </span>
          <input
            value={dependsOn}
            onChange={(event) => setDependsOn(event.target.value)}
            placeholder="T-0001, T-0002"
            className={inputClass}
          />
        </label>
      </div>
      <footer className="flex h-13 shrink-0 items-center justify-end gap-2 border-t border-line bg-surface px-4">
        <Button variant="ghost" onClick={onCancel}>
          Cancel
        </Button>
        <Button
          type="submit"
          variant="primary"
          disabled={busy || title.trim() === ''}
          shortcut="Mod+Enter"
        >
          Save
        </Button>
      </footer>
    </form>
  )
}
