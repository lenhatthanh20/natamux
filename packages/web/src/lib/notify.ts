import { needsMe, type Task, type TaskState } from '@natamux/core'
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

// Tasks that became done since the last look. A cancel is always someone's own act, so it does not
// ring.
export function tasksThatFinished(
  previous: ReadonlyMap<string, TaskState>,
  tasks: readonly Task[],
): Task[] {
  return tasks.filter((task) => {
    const before = previous.get(task.id)
    return before !== undefined && before !== 'done' && task.state === 'done'
  })
}

// The toast for the task on screen when it finishes.
export function finishedText(task: Pick<Task, 'id' | 'state'>): string {
  return task.state === 'cancelled' ? `${task.id} was cancelled.` : `${task.id} is done.`
}
