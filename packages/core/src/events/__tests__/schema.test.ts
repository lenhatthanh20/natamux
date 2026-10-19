import { describe, expect, it } from 'vitest'
import { GATE_EVENT_TYPES, taskEventSchema } from '../schema.js'

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

describe('GATE_EVENT_TYPES', () => {
  // A change here changes what an agent may do by itself, so it must be on purpose.
  it('holds exactly the events that pass a gate or set a state by hand', () => {
    expect([...GATE_EVENT_TYPES].sort()).toEqual([
      'review.approved',
      'stage.approved',
      'stage.passed',
      'stage.skipped',
      'state.overridden',
    ])
  })
})
