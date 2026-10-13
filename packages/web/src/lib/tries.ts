import type { Task } from '@natamux/core'
import { taskNumber } from './tasks'

const TRY_LABEL = /^tries-[a-z]+-\d+$/

// The label that groups a task with its tries (see fan-out in the daemon), or null.
export function tryGroup(task: Pick<Task, 'labels'>): string | null {
  return task.labels.find((label) => TRY_LABEL.test(label)) ?? null
}

// Every task of the group, the first try first. The group is named after the first try, whose
// number may be higher than the others when the user chose its id.
export function triesOf(group: string, tasks: readonly Task[]): Task[] {
  const first = (task: Task) => (group === `tries-${task.id.toLowerCase()}` ? 0 : 1)
  return tasks
    .filter((task) => task.labels.includes(group))
    .sort((a, b) => first(a) - first(b) || taskNumber(a.id) - taskNumber(b.id))
}

// The tries to cancel when one is kept: the others that are not finished yet.
export function triesToCancel(kept: Task, tries: readonly Task[]): Task[] {
  return tries.filter(
    (task) => task.id !== kept.id && task.state !== 'done' && task.state !== 'cancelled',
  )
}
