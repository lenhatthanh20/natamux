import { TASK_PRIORITIES, type Task } from '@helmsman/core'
import { type FormEvent, useState } from 'react'
import type { TaskChanges } from '../lib/api'

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

const inputClass = 'w-full rounded-md border border-line bg-canvas px-2 py-1.5'

export function TaskEditForm({ task, busy, onSave, onCancel }: TaskEditFormProps) {
  const [title, setTitle] = useState(task.title)
  const [description, setDescription] = useState(task.description)
  const [priority, setPriority] = useState(task.priority)
  const [labels, setLabels] = useState(task.labels.join(', '))

  const submit = (event: FormEvent) => {
    event.preventDefault()
    if (title.trim() === '') {
      return
    }
    onSave({ title: title.trim(), description, priority, labels: parseLabels(labels) })
  }

  return (
    <form onSubmit={submit} aria-label={`Edit ${task.id}`} className="flex flex-col gap-3">
      <label className="flex flex-col gap-1">
        <span className="text-muted">Title</span>
        <input
          value={title}
          onChange={(event) => setTitle(event.target.value)}
          className={inputClass}
        />
      </label>
      <label className="flex flex-col gap-1">
        <span className="text-muted">Notes (markdown)</span>
        <textarea
          rows={6}
          value={description}
          onChange={(event) => setDescription(event.target.value)}
          className={`${inputClass} font-mono`}
        />
      </label>
      <div className="flex gap-3">
        <label className="flex flex-1 flex-col gap-1">
          <span className="text-muted">Priority</span>
          <select
            value={priority}
            onChange={(event) => setPriority(event.target.value as Task['priority'])}
            className={inputClass}
          >
            {TASK_PRIORITIES.map((value) => (
              <option key={value} value={value}>
                {value}
              </option>
            ))}
          </select>
        </label>
        <label className="flex flex-[2] flex-col gap-1">
          <span className="text-muted">Labels, comma separated</span>
          <input
            value={labels}
            onChange={(event) => setLabels(event.target.value)}
            className={inputClass}
          />
        </label>
      </div>
      <div className="flex gap-2">
        <button
          type="submit"
          disabled={busy || title.trim() === ''}
          className="rounded-md bg-accent px-3 py-1.5 font-medium text-surface disabled:opacity-50"
        >
          Save
        </button>
        <button type="button" onClick={onCancel} className="px-3 py-1.5 text-muted">
          Cancel
        </button>
      </div>
    </form>
  )
}
