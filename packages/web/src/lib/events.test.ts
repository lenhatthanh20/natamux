import { EVENT_TYPES, type EventType, type TaskEvent } from '@helmsman/core'
import { describe, expect, it } from 'vitest'
import type { StoredEvent } from './api'
import { actorOf, describeEvent, describeLine, formatClock, groupEvents } from './events'

describe('describeEvent', () => {
  it('has a sentence for every kind of event', () => {
    for (const type of EVENT_TYPES) {
      expect(describeEvent(type).length, type).toBeGreaterThan(0)
    }
  })

  it('does not give two kinds of event the same sentence', () => {
    const sentences = EVENT_TYPES.map((type) => describeEvent(type))
    expect(new Set(sentences).size).toBe(sentences.length)
  })
})

describe('formatClock', () => {
  it('shows hours, minutes and seconds', () => {
    expect(formatClock('2026-10-08T09:05:07.000Z')).toMatch(/\d{1,2}\D\d{2}\D\d{2}/)
  })
})

function stored(
  seq: number,
  type: EventType,
  source: TaskEvent['source'] = 'human',
  accepted = true,
): StoredEvent {
  const at = `2026-10-08T09:00:${String(seq).padStart(2, '0')}.000Z`
  return { seq, accepted, event: { taskId: 'T-0001', source, type, at, payload: {} } }
}

describe('actorOf', () => {
  it('names the user, the agent and the daemon', () => {
    expect(actorOf('human')).toBe('You')
    expect(actorOf('agent')).toBe('Agent')
    expect(actorOf('system')).toBe('Daemon')
    expect(actorOf('git')).toBe('Daemon')
  })
})

describe('groupEvents', () => {
  it('joins events of the same kind in a row and counts them', () => {
    const lines = groupEvents([
      stored(1, 'task.created', 'system'),
      stored(2, 'task.edited'),
      stored(3, 'task.edited'),
      stored(4, 'task.edited'),
      stored(5, 'task.edited'),
    ])
    expect(lines.map(describeLine)).toEqual(['Task created · Daemon', 'Details edited ×4 · You'])
    expect(lines[1]?.at).toBe('2026-10-08T09:00:05.000Z')
  })

  it('keeps events apart when another event comes between them', () => {
    const lines = groupEvents([
      stored(1, 'task.edited'),
      stored(2, 'git.committed', 'git'),
      stored(3, 'task.edited'),
    ])
    expect(lines).toHaveLength(3)
  })

  it('keeps events apart when the actor or the answer of the daemon differs', () => {
    expect(groupEvents([stored(1, 'task.edited'), stored(2, 'task.edited', 'agent')])).toHaveLength(
      2,
    )
    expect(
      groupEvents([
        stored(1, 'agent.resumed', 'human'),
        stored(2, 'agent.resumed', 'human', false),
      ]),
    ).toHaveLength(2)
  })

  it('has no lines without events', () => {
    expect(groupEvents([])).toEqual([])
  })
})
