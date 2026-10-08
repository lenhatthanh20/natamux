import { type Task, type TaskState, taskSchema } from '@helmsman/core'
import { describe, expect, it } from 'vitest'
import { triesOf, triesToCancel, tryGroup } from './tries'

// Through the schema, so new task fields get their defaults here too.
function task(id: string, labels: string[], state: TaskState = 'running'): Task {
  return taskSchema.parse({
    id,
    title: id,
    state,
    labels,
    createdAt: '2026-10-08T09:00:00.000Z',
    updatedAt: '2026-10-08T09:00:00.000Z',
  })
}

describe('tries', () => {
  const group = 'tries-t-0007'
  const all = [
    task('T-0010', [group], 'cancelled'),
    task('T-0007', ['perf', group]),
    task('T-0008', [group], 'review'),
    task('T-0009', ['other']),
  ]

  it('finds the group of a task', () => {
    expect(tryGroup(all[1] as Task)).toBe(group)
    expect(tryGroup(all[3] as Task)).toBeNull()
  })

  it('lists the tries of a group in order', () => {
    expect(triesOf(group, all).map((entry) => entry.id)).toEqual(['T-0007', 'T-0008', 'T-0010'])
  })

  it('cancels the other tries that are not finished', () => {
    const tries = triesOf(group, all)
    expect(triesToCancel(tries[1] as Task, tries).map((entry) => entry.id)).toEqual(['T-0007'])
  })
})
