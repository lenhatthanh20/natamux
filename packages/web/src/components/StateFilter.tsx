import { TASK_STATES, type Task, type TaskState } from '@helmsman/core'
import { countByState, STATE_LABEL } from '../lib/tasks'

type StateFilterProps = {
  tasks: readonly Task[]
  active: TaskState | null
  onToggle: (state: TaskState) => void
}

export function StateFilter({ tasks, active, onToggle }: StateFilterProps) {
  const counts = countByState(tasks)

  return (
    <ul className="flex flex-wrap gap-2" aria-label="Filter by state">
      {TASK_STATES.map((state) => (
        <li key={state}>
          <button
            type="button"
            aria-pressed={active === state}
            onClick={() => onToggle(state)}
            disabled={counts[state] === 0 && active !== state}
            className="flex items-center gap-2 rounded-md border border-line bg-surface px-2 py-1 hover:border-accent disabled:opacity-40 aria-pressed:border-accent aria-pressed:ring-1 aria-pressed:ring-accent"
          >
            <span className="font-semibold">{counts[state]}</span>
            <span className="text-muted">{STATE_LABEL[state]}</span>
          </button>
        </li>
      ))}
    </ul>
  )
}
