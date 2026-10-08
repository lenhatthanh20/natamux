import type { Task } from '@helmsman/core'
import { describe, expect, it } from 'vitest'
import { type DaemonMessage, EventBus } from './event-bus.js'

const task: Task = {
  id: 'T-0001',
  title: 'Example',
  description: '',
  state: 'draft',
  priority: 'normal',
  labels: [],
  agent: null,
  branch: null,
  workspace: null,
  pr: null,
  prStatus: null,
  parentId: null,
  repo: null,
  dependsOn: [],
  createdAt: '2026-10-08T09:00:00.000Z',
  updatedAt: '2026-10-08T09:00:00.000Z',
}

const message: DaemonMessage = { kind: 'task.created', task }

describe('EventBus', () => {
  it('delivers a message to every subscriber', () => {
    const bus = new EventBus()
    const first: DaemonMessage[] = []
    const second: DaemonMessage[] = []
    bus.subscribe((received) => first.push(received))
    bus.subscribe((received) => second.push(received))

    bus.publish(message)

    expect(first).toEqual([message])
    expect(second).toEqual([message])
  })

  it('stops delivering after unsubscribe', () => {
    const bus = new EventBus()
    const received: DaemonMessage[] = []
    const unsubscribe = bus.subscribe((next) => received.push(next))

    unsubscribe()
    bus.publish(message)

    expect(received).toEqual([])
  })

  it('keeps delivering when one subscriber throws', () => {
    const bus = new EventBus()
    const received: DaemonMessage[] = []
    bus.subscribe(() => {
      throw new Error('broken listener')
    })
    bus.subscribe((next) => received.push(next))

    bus.publish(message)

    expect(received).toEqual([message])
  })
})
