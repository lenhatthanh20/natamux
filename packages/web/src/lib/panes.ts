import type { Task } from '@helmsman/core'

// More panes than this make each terminal too small to read.
export const MAX_PANES = 6

// Columns of the pane grid: side by side up to three, then two rows.
export function paneColumns(count: number): number {
  if (count <= 3) {
    return Math.max(count, 1)
  }
  return count === 4 ? 2 : 3
}

function hasAgent(task: Task): boolean {
  return task.workspace !== null && (task.state === 'running' || task.state === 'needs-input')
}

// The tasks in the agents grid. Until the user picks, the ones whose agent works or waits.
export function agentPaneTasks(chosen: readonly string[] | null, tasks: readonly Task[]): Task[] {
  if (chosen === null) {
    return tasks.filter(hasAgent).slice(0, MAX_PANES)
  }
  return chosen
    .map((id) => tasks.find((task) => task.id === id))
    .filter((task): task is Task => task !== undefined && task.workspace !== null)
}
