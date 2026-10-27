import { EVENT_TYPES, type EventType, type TaskEvent } from '@natamux/core'
import { describe, expect, it } from 'vitest'
import type { StoredEvent } from '../../../lib/api'
import { actorOf, describeEvent, describeLine, formatClock, groupEvents } from '../task-events'

describe('describeEvent', () => {
  it('has a sentence for every kind of event', () => {
    for (const type of EVENT_TYPES) {
      expect(describeEvent(type).length, type).toBeGreaterThan(0)
    }
  })

  it('names a change of the skip mark, and a planned skip as any skip', () => {
    expect(describeEvent('stage.skip-set')).toBe('Skip choice changed')
    expect(describeEvent('stage.skipped')).toBe('Stage skipped')
  })

  it('does not give two kinds of event the same sentence', () => {
    const sentences = EVENT_TYPES.map((type) => describeEvent(type))
    expect(new Set(sentences).size).toBe(sentences.length)
  })

  it('says a send back by CI came from CI, also when it goes to the same stage', () => {
    const fromCi = { to: 'implement', from: 'implement', byAgent: true, ci: true }
    expect(describeEvent('stage.reopened', fromCi)).toBe('Sent back by CI')
    expect(describeEvent('stage.reopened', { to: 'plan', reason: 'x' })).toBe(
      'Sent back to an earlier stage',
    )
    expect(describeEvent('stage.started', { ci: true })).toBe('Stage started')
  })

  it('says how long a paused agent was idle', () => {
    expect(describeEvent('agent.paused', { terminal: '1', idleMinutes: 30 })).toBe(
      'Agent paused after 30 min idle',
    )
    expect(describeEvent('agent.paused')).toBe('Agent paused while idle')
  })
})

describe('describeEvent with a path', () => {
  it('ends with an arrow and the name of the path an event chose', () => {
    const payload = { stage: 'plan', path: 'small' }
    expect(describeEvent('stage.approved', payload, { small: 'Small change' })).toBe(
      'Stage approved → Small change',
    )
  })

  it('names the path by its id when no name is known, and adds nothing without a path', () => {
    expect(describeEvent('stage.approved', { path: 'small' })).toBe('Stage approved → small')
    expect(describeEvent('stage.approved', { stage: 'plan' }, { small: 'Small change' })).toBe(
      'Stage approved',
    )
  })

  it('shows the path in grouped lines', () => {
    const event = stored(1, 'stage.approved')
    event.event.payload = { path: 'small' }
    const [line] = groupEvents([event], { small: 'Small change' })
    expect(describeLine(line as never)).toBe('Stage approved → Small change · You')
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

  it('keeps a send back by CI apart from one by a person', () => {
    const reopened = (seq: number, payload: Record<string, unknown>): StoredEvent => {
      const base = stored(seq, 'stage.reopened', 'system')
      return { ...base, event: { ...base.event, payload } }
    }
    const lines = groupEvents([
      reopened(1, { to: 'implement', ci: true }),
      reopened(2, { to: 'implement', ci: true }),
      reopened(3, { to: 'implement' }),
    ])
    expect(lines.map(describeLine)).toEqual([
      'Sent back by CI ×2 · Daemon',
      'Sent back to an earlier stage · Daemon',
    ])
  })

  it('says why a workspace could not be made', () => {
    const base = stored(1, 'workspace.failed', 'system')
    const failed = { ...base, event: { ...base.event, payload: { message: 'index.lock exists' } } }

    expect(groupEvents([failed])[0]?.detail).toBe('index.lock exists')
  })

  it('has no lines without events', () => {
    expect(groupEvents([])).toEqual([])
  })

  it('lists all the background work the agent waits for, from the newest event of a run', () => {
    const waiting = (seq: number, waitingFor: unknown): StoredEvent => {
      const base = stored(seq, 'agent.waiting', 'agent')
      return { ...base, event: { ...base.event, payload: { waitingFor } } }
    }
    const lines = groupEvents([
      waiting(1, ['Bash: pnpm lint']),
      waiting(2, ['Bash: pnpm test', 'subagent: Review diff', 'scheduled prompt: check CI']),
    ])

    expect(lines.map(describeLine)).toEqual([
      'Agent waits for its background work ×2: Bash: pnpm test; subagent: Review diff; ' +
        'scheduled prompt: check CI · Agent',
    ])
    expect(lines[0]?.detail).toBe(
      'Bash: pnpm test; subagent: Review diff; scheduled prompt: check CI',
    )
  })

  it('has no detail for other events, or for a list it cannot read', () => {
    const odd = stored(1, 'agent.waiting', 'agent')
    const lines = groupEvents([
      { ...odd, event: { ...odd.event, payload: { waitingFor: 'Bash: ls' } } },
      stored(2, 'agent.finished', 'agent'),
    ])
    expect(lines.map((line) => line.detail)).toEqual([undefined, undefined])
  })
})
