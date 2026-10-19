import type { Task } from '@natamux/core'
import { type FormEvent, useState } from 'react'
import { DetailSection } from '../../components/DetailSection'
import { StateBadge } from '../../components/StateBadge'
import { Button } from '../../components/ui/Button'
import { input } from '../../components/ui/styles'

type SubtasksProps = {
  subtasks: readonly Task[]
  busy: boolean
  onSelect: (id: string) => void
  onAdd: (title: string) => void
}

// The parts of a task. Each is a task of its own, with its own state and workspace. A subtask
// starts from the parent's branch, and its pull request goes into that branch.
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
    <DetailSection
      id="subtasks"
      title="Subtasks"
      aside={
        subtasks.length > 0 ? (
          <span className="text-xs text-muted">
            {done} of {subtasks.length} done
          </span>
        ) : undefined
      }
    >
      {subtasks.length > 0 && (
        <ul className="flex flex-col">
          {subtasks.map((task) => (
            <li key={task.id}>
              <button
                type="button"
                onClick={() => onSelect(task.id)}
                className="focus-inset -mx-2 flex h-7 w-[calc(100%+1rem)] items-center gap-2 rounded-md px-2 text-left hover:bg-hover"
              >
                <span className="font-mono text-xs text-muted">{task.id}</span>
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
          className={`${input} flex-1`}
        />
        <Button type="submit" disabled={busy || title.trim() === ''}>
          Add
        </Button>
      </form>
    </DetailSection>
  )
}
