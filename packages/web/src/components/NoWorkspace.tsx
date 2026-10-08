import type { Task } from '@helmsman/core'

type NoWorkspaceProps = {
  task: Task
  busy: boolean
  // Claude Code cannot start while the task waits for others, or once it is finished.
  canStart: boolean
  onStartAgent: () => void
}

// What the main area shows for a task that has no worktree yet.
export function NoWorkspace({ task, busy, canStart, onStartAgent }: NoWorkspaceProps) {
  return (
    <section
      aria-label={`Workspace of ${task.id}`}
      className="flex flex-1 flex-col items-center justify-center gap-3 p-8 text-center"
    >
      <p className="font-mono text-muted">{task.id}</p>
      <h2 className="text-base font-semibold">{task.title}</h2>
      <p className="max-w-sm text-muted">
        This task has no workspace yet. Start Claude Code to make one and run the agent in it.
      </p>
      {canStart && (
        <button
          type="button"
          disabled={busy}
          onClick={onStartAgent}
          className="rounded-md bg-accent px-3 py-1.5 font-medium text-white disabled:opacity-50"
        >
          Start Claude Code
        </button>
      )}
    </section>
  )
}
