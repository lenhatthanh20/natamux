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
