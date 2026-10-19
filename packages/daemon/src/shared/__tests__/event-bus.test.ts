import type { Task } from '@natamux/core'
import { describe, expect, it, vi } from 'vitest'
import { type DaemonMessage, EventBus } from '../event-bus.js'

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
  stage: 'implement',
  stages: ['implement', 'review', 'release'],
  lanes: {},
  paths: {},
  stageSince: null,
  backgroundSince: null,
  workflowStages: [],
  context: null,
  issue: null,
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

  it('keeps delivering when one subscriber throws, and logs the error', () => {
    const bus = new EventBus()
    const received: DaemonMessage[] = []
    bus.subscribe(() => {
      throw new Error('broken listener')
    })
    bus.subscribe((next) => received.push(next))
    const error = vi.spyOn(console, 'error').mockImplementation(() => undefined)
    try {
      bus.publish(message)
      expect(error).toHaveBeenCalledOnce()
      expect(error).toHaveBeenCalledWith(expect.stringContaining('broken listener'))
    } finally {
      error.mockRestore()
    }

    expect(received).toEqual([message])
  })
})

describe('EventBus clients', () => {
  it('counts only the dashboards, not the services that subscribe', () => {
    const bus = new EventBus()
    bus.subscribe(() => undefined)
    expect(bus.size()).toBe(0)

    const first = bus.subscribeClient(() => undefined)
    const second = bus.subscribeClient(() => undefined)
    expect(bus.size()).toBe(2)

    first()
    second()
    expect(bus.size()).toBe(0)
  })

  it('makes the JSON text once for all dashboards', () => {
    const bus = new EventBus()
    const texts: string[] = []
    for (let count = 0; count < 3; count += 1) {
      bus.subscribeClient((text) => texts.push(text))
    }
    const services: DaemonMessage[] = []
    bus.subscribe((next) => services.push(next))
    const stringify = vi.spyOn(JSON, 'stringify')
    try {
      bus.publish(message)
      expect(stringify.mock.calls.filter(([value]) => value === message)).toHaveLength(1)
    } finally {
      stringify.mockRestore()
    }

    expect(texts).toEqual([0, 1, 2].map(() => JSON.stringify(message)))
    expect(services).toEqual([message])
  })

  it('keeps sending when one dashboard throws, and logs the error', () => {
    const bus = new EventBus()
    const texts: string[] = []
    bus.subscribeClient(() => {
      throw new Error('closed socket')
    })
    bus.subscribeClient((text) => texts.push(text))
    const error = vi.spyOn(console, 'error').mockImplementation(() => undefined)
    try {
      bus.publish(message)
      expect(error).toHaveBeenCalledOnce()
      expect(error).toHaveBeenCalledWith(expect.stringContaining('closed socket'))
    } finally {
      error.mockRestore()
    }

    expect(texts).toEqual([JSON.stringify(message)])
  })

  it('makes no text without dashboards', () => {
    const bus = new EventBus()
    bus.subscribe(() => undefined)
    const stringify = vi.spyOn(JSON, 'stringify')
    try {
      bus.publish(message)
      expect(stringify.mock.calls.filter(([value]) => value === message)).toHaveLength(0)
    } finally {
      stringify.mockRestore()
    }
  })

  it('keeps sending to the other dashboards when one throws', () => {
    const bus = new EventBus()
    const texts: string[] = []
    bus.subscribeClient(() => {
      throw new Error('closed socket')
    })
    bus.subscribeClient((text) => texts.push(text))

    bus.publish({ kind: 'shells.changed', taskId: 'T-0001' })

    expect(texts).toEqual(['{"kind":"shells.changed","taskId":"T-0001"}'])
  })
})
