import type { Task, TerminalSize } from '@helmsman/core'
import { agentPaneTasks, MAX_PANES, paneColumns } from '../lib/panes'
import { TerminalPane } from './TerminalPane'
import { Select } from './ui/Select'

type AgentPanesProps = {
  tasks: readonly Task[]
  // The task ids the user picked, or null for the default: the agents that work or wait.
  chosen: readonly string[] | null
  onChange: (ids: string[]) => void
  connect: (taskId: string, size: TerminalSize) => Promise<WebSocket>
  onOpenTask: (id: string) => void
}

// Several agents at once: shell 1 of each task, in a grid.
export function AgentPanes({ tasks, chosen, onChange, connect, onOpenTask }: AgentPanesProps) {
  const shown = agentPaneTasks(chosen, tasks)
  const ids = shown.map((task) => task.id)
  const others = tasks.filter(
    (task) =>
      task.workspace !== null &&
      task.state !== 'done' &&
      task.state !== 'cancelled' &&
      !ids.includes(task.id),
  )

  return (
    <section aria-label="Agents" className="flex min-h-0 flex-1 flex-col gap-2 p-2">
      <header className="flex h-9 items-center gap-3 px-2">
        <h2 className="font-semibold">Agents</h2>
        <span className="text-muted">
          {shown.length} of {MAX_PANES} panes
        </span>
        <span className="flex-1" />
        {others.length > 0 && shown.length < MAX_PANES && (
          <Select
            aria-label="Add a pane"
            value=""
            onChange={(event) => {
              if (event.target.value !== '') {
                onChange([...ids, event.target.value])
              }
            }}
            className="w-64"
          >
            <option value="">Add a pane</option>
            {others.map((task) => (
              <option key={task.id} value={task.id}>
                {task.id} {task.title}
              </option>
            ))}
          </Select>
        )}
      </header>
      {shown.length === 0 ? (
        <p className="m-auto max-w-sm text-center text-muted">
          No agent works or waits now. Start Claude Code on a task, or add a task that has a
          workspace.
        </p>
      ) : (
        <div
          className="grid min-h-0 flex-1 auto-rows-[minmax(12rem,1fr)] gap-2 overflow-auto"
          style={{ gridTemplateColumns: `repeat(${paneColumns(shown.length)}, minmax(0, 1fr))` }}
        >
          {shown.map((task) => (
            <TerminalPane
              key={task.id}
              label={`Agent of ${task.id}`}
              header={
                <>
                  <span className="state-dot" data-state={task.state} aria-hidden="true" />
                  <button
                    type="button"
                    onClick={() => onOpenTask(task.id)}
                    title="Open the workspace"
                    className="min-w-0 truncate text-left font-medium hover:underline"
                  >
                    <span className="font-mono font-normal text-muted">{task.id}</span> {task.title}
                  </button>
                </>
              }
              connect={(size) => connect(task.id, size)}
              onClose={() => onChange(ids.filter((id) => id !== task.id))}
            />
          ))}
        </div>
      )}
    </section>
  )
}
