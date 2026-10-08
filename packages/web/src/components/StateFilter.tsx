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
    <ul className="flex min-w-0 gap-0.5 overflow-x-auto [scrollbar-width:none]" aria-label="Filter by state">
      {TASK_STATES.filter((state) => counts[state] > 0 || active === state).map((state) => (
        <li key={state}>
          <button
            type="button"
            aria-pressed={active === state}
            onClick={() => onToggle(state)}
            className="flex h-7 items-center gap-1.5 rounded-md px-2 whitespace-nowrap text-muted hover:bg-hover hover:text-ink aria-pressed:bg-selected aria-pressed:text-ink"
          >
            <span className="state-dot" data-state={state} aria-hidden="true" />
            <span>{STATE_LABEL[state]}</span>
            <span className="text-xs tabular-nums opacity-70">{counts[state]}</span>
          </button>
        </li>
      ))}
    </ul>
  )
}
