import type { Task } from './task.js'

// The tasks this task waits for: those it depends on that are not done. A task that does not
// exist (any more) is not waited for.
export function waitingOn(
  task: Pick<Task, 'dependsOn'>,
  tasks: readonly Pick<Task, 'id' | 'state'>[],
): string[] {
  const states = new Map(tasks.map((other) => [other.id, other.state]))
  return task.dependsOn.filter((id) => {
    const state = states.get(id)
    return state !== undefined && state !== 'done'
  })
}

// The subtasks of this task that are not finished. Its next stage waits for them, as their work
// is merged into its branch.
export function openSubtasks(
  task: Pick<Task, 'id'>,
  tasks: readonly Pick<Task, 'id' | 'state' | 'parentId'>[],
): string[] {
  return tasks
    .filter(
      (other) =>
        other.parentId === task.id && other.state !== 'done' && other.state !== 'cancelled',
    )
    .map((other) => other.id)
}

// What people see the task wait for: its dependencies, and its open subtasks while a stage has
// not started. A stage that already runs is not stopped by a new subtask.
export function waitsFor(
  task: Pick<Task, 'id' | 'state' | 'dependsOn'>,
  tasks: readonly Pick<Task, 'id' | 'state' | 'parentId'>[],
): string[] {
  const notStarted = task.state === 'ready' || task.state === 'draft'
  const subtasks = notStarted ? openSubtasks(task, tasks) : []
  return [...waitingOn(task, tasks), ...subtasks]
}

// True when `taskId` depending on `dependsOn` would make a loop: one of them already depends,
// directly or through others, on `taskId`.
export function makesCycle(
  taskId: string,
  dependsOn: readonly string[],
  tasks: readonly Pick<Task, 'id' | 'dependsOn'>[],
): boolean {
  const edges = new Map(tasks.map((task) => [task.id, task.dependsOn]))
  const seen = new Set<string>()
  const stack = [...dependsOn]
  while (stack.length > 0) {
    const id = stack.pop() ?? ''
    if (id === taskId) {
      return true
    }
    if (seen.has(id)) {
      continue
    }
    seen.add(id)
    stack.push(...(edges.get(id) ?? []))
  }
  return false
}
