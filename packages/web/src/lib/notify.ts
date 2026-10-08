import { needsMe, type Task, type TaskState } from '@helmsman/core'
import { STATE_LABEL } from './tasks'

// Tasks that moved into a state that needs the user since the last look. A task seen for the first
// time does not count, so opening the dashboard does not ring for every waiting task.
export function tasksThatStartedWaiting(
  previous: ReadonlyMap<string, TaskState>,
  tasks: readonly Task[],
): Task[] {
  return tasks.filter((task) => {
    const before = previous.get(task.id)
    return before !== undefined && needsMe(task.state) && !needsMe(before)
  })
}

export function statesById(tasks: readonly Task[]): Map<string, TaskState> {
  return new Map(tasks.map((task) => [task.id, task.state]))
}

export function notificationFor(task: Task): { title: string; body: string } {
  return { title: `${task.id}: ${STATE_LABEL[task.state]}`, body: task.title }
}
