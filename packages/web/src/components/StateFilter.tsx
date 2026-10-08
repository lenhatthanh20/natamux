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
    <ul className="flex flex-wrap gap-1" aria-label="Filter by state">
      {TASK_STATES.filter((state) => counts[state] > 0 || active === state).map((state) => (
        <li key={state}>
          <button
            type="button"
            aria-pressed={active === state}
            onClick={() => onToggle(state)}
            className="flex items-center gap-1.5 rounded-md px-2 py-0.5 text-muted hover:bg-hover hover:text-ink aria-pressed:bg-selected aria-pressed:text-ink"
          >
            <span className="state-dot" data-state={state} aria-hidden="true" />
            <span>{STATE_LABEL[state]}</span>
            <span className="font-semibold">{counts[state]}</span>
          </button>
        </li>
      ))}
    </ul>
  )
}
