import { describe, expect, it } from 'vitest'
import {
  applyTaskEvent,
  deriveTaskState,
  type EventType,
  type TaskEvent,
  taskEventSchema,
} from './events.js'

function event(type: EventType, payload: Record<string, unknown> = {}): TaskEvent {
  return { taskId: 'T-1', source: 'system', type, at: '2026-10-08T09:00:00Z', payload }
}

describe('deriveTaskState', () => {
  it('starts in draft with no events', () => {
    expect(deriveTaskState([])).toEqual({ state: 'draft', rejected: [] })
  })

  it('stays in draft after task.created', () => {
    expect(deriveTaskState([event('task.created')]).state).toBe('draft')
  })

  it('follows a normal run to done', () => {
    const result = deriveTaskState([
      event('task.created'),
      event('task.readied'),
      event('agent.started'),
      event('agent.asked'),
      event('agent.resumed'),
      event('agent.finished'),
      event('review.approved'),
    ])
    expect(result).toEqual({ state: 'done', rejected: [] })
  })

  it('goes back to running when changes are requested', () => {
    const result = deriveTaskState([
      event('task.readied'),
      event('agent.started'),
      event('agent.finished'),
      event('review.changes-requested'),
    ])
    expect(result.state).toBe('running')
  })

  it('moves a failed task to ready on retry', () => {
    const result = deriveTaskState([
      event('task.readied'),
      event('agent.started'),
      event('agent.failed'),
      event('task.retried'),
    ])
    expect(result.state).toBe('ready')
  })

  it('skips an event that does not fit and reports it', () => {
    const early = event('agent.finished')
    const result = deriveTaskState([event('task.readied'), early])
    expect(result.state).toBe('ready')
    expect(result.rejected).toEqual([early])
  })

  it('keeps a terminal state against later events', () => {
    const late = event('agent.started')
    const result = deriveTaskState([event('task.cancelled'), late])
    expect(result.state).toBe('cancelled')
    expect(result.rejected).toEqual([late])
  })

  it('lets a human override jump to any valid state', () => {
    const result = deriveTaskState([
      event('task.cancelled'),
      event('state.overridden', { to: 'ready' }),
    ])
    expect(result).toEqual({ state: 'ready', rejected: [] })
  })

  it('rejects an override with an unknown target state', () => {
    const bad = event('state.overridden', { to: 'finished' })
    const result = deriveTaskState([event('task.readied'), bad])
    expect(result.state).toBe('ready')
    expect(result.rejected).toEqual([bad])
  })

  it('rejects an override with no target state', () => {
    const bad = event('state.overridden')
    expect(deriveTaskState([bad]).rejected).toEqual([bad])
  })
})

describe('applyTaskEvent', () => {
  it('accepts an event that fits the state and returns the new state', () => {
    expect(applyTaskEvent('ready', event('agent.started'))).toEqual({
      state: 'running',
      accepted: true,
    })
  })

  it('keeps the state when the event does not fit', () => {
    expect(applyTaskEvent('ready', event('agent.finished'))).toEqual({
      state: 'ready',
      accepted: false,
    })
  })

  it('accepts task.created without changing the state', () => {
    expect(applyTaskEvent('draft', event('task.created'))).toEqual({
      state: 'draft',
      accepted: true,
    })
  })

  it.each([
    'task.created',
    'workspace.created',
    'workspace.removed',
    'agent.active',
    'git.committed',
    'git.pushed',
    'pr.opened',
    'agent.launched',
  ] as const)('accepts %s in any state without changing it', (type) => {
    expect(applyTaskEvent('running', event(type))).toEqual({ state: 'running', accepted: true })
    expect(applyTaskEvent('done', event(type))).toEqual({ state: 'done', accepted: true })
  })

  it('accepts an override to any valid state', () => {
    expect(applyTaskEvent('done', event('state.overridden', { to: 'running' }))).toEqual({
      state: 'running',
      accepted: true,
    })
  })

  it('does not accept an override to an unknown state', () => {
    expect(applyTaskEvent('done', event('state.overridden', { to: 'nope' }))).toEqual({
      state: 'done',
      accepted: false,
    })
  })
})

describe('taskEventSchema', () => {
  it('fills an empty payload by default', () => {
    const parsed = taskEventSchema.parse({
      taskId: 'T-1',
      source: 'agent',
      type: 'agent.started',
      at: '2026-10-08T09:00:00Z',
    })
    expect(parsed.payload).toEqual({})
  })

  it('rejects an unknown event type', () => {
    const result = taskEventSchema.safeParse({
      taskId: 'T-1',
      source: 'agent',
      type: 'agent.exploded',
      at: '2026-10-08T09:00:00Z',
    })
    expect(result.success).toBe(false)
  })

  it('rejects a timestamp that is not ISO 8601', () => {
    const result = taskEventSchema.safeParse({
      taskId: 'T-1',
      source: 'agent',
      type: 'agent.started',
      at: 'yesterday',
    })
    expect(result.success).toBe(false)
  })
})
