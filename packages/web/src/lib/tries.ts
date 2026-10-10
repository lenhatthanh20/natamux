import type { Task } from '@natamux/core'

const TRY_LABEL = /^tries-t-\d+$/

// The label that groups a task with its tries (see fan-out in the daemon), or null.
export function tryGroup(task: Pick<Task, 'labels'>): string | null {
  return task.labels.find((label) => TRY_LABEL.test(label)) ?? null
}

// Every task of the group, the first try first.
export function triesOf(group: string, tasks: readonly Task[]): Task[] {
  return tasks
    .filter((task) => task.labels.includes(group))
    .sort((a, b) => Number(a.id.slice(2)) - Number(b.id.slice(2)))
}

// The tries to cancel when one is kept: the others that are not finished yet.
export function triesToCancel(kept: Task, tries: readonly Task[]): Task[] {
  return tries.filter(
    (task) => task.id !== kept.id && task.state !== 'done' && task.state !== 'cancelled',
  )
}
