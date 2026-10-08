import type { Task } from '@helmsman/core'
import { type FormEvent, useState } from 'react'
import { StateBadge } from './StateBadge'

type SubtasksProps = {
  subtasks: readonly Task[]
  busy: boolean
  onSelect: (id: string) => void
  onAdd: (title: string) => void
}

// The parts of a task. Each is a task of its own, with its own state and workspace.
export function Subtasks({ subtasks, busy, onSelect, onAdd }: SubtasksProps) {
  const [title, setTitle] = useState('')
  const done = subtasks.filter((task) => task.state === 'done').length

  const submit = (event: FormEvent) => {
    event.preventDefault()
    if (title.trim() !== '') {
      onAdd(title.trim())
      setTitle('')
    }
  }

  return (
    <section aria-label="Subtasks" className="flex flex-col gap-2">
      <h3 className="font-semibold">
        Subtasks
        {subtasks.length > 0 && (
          <span className="ml-2 font-normal text-muted">
            {done} of {subtasks.length} done
          </span>
        )}
      </h3>
      {subtasks.length > 0 && (
        <ul className="flex flex-col">
          {subtasks.map((task) => (
            <li key={task.id}>
              <button
                type="button"
                onClick={() => onSelect(task.id)}
                className="flex w-full items-baseline gap-2 py-0.5 text-left hover:text-accent"
              >
                <span className="font-mono text-muted">{task.id}</span>
                <span className="min-w-0 flex-1 truncate">{task.title}</span>
                <StateBadge state={task.state} />
              </button>
            </li>
          ))}
        </ul>
      )}
      <form onSubmit={submit} className="flex gap-2">
        <input
          value={title}
          onChange={(event) => setTitle(event.target.value)}
          placeholder="Add a subtask"
          aria-label="New subtask title"
          className="min-w-0 flex-1 rounded-md border border-line bg-canvas px-2 py-1"
        />
        <button
          type="submit"
          disabled={busy || title.trim() === ''}
          className="rounded-md border border-line px-2 py-1 font-medium hover:border-accent disabled:opacity-50"
        >
          Add
        </button>
      </form>
    </section>
  )
}
