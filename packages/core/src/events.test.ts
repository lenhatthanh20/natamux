import { describe, expect, it } from 'vitest'
import {
  applyTaskEvent,
  completeEvent,
  deriveTaskState,
  type EventType,
  GATE_EVENT_TYPES,
  type TaskEvent,
  taskEventSchema,
} from './events.js'
import { DEFAULT_PIPELINE, LEGACY_PIPELINE, type StagePosition, type Step } from './stage.js'

// The pipeline these cases were written for: a person reviews, then the tests run. The default
// pipeline has an agent review after the tests instead.
const PIPELINE: Step[] = DEFAULT_PIPELINE.flatMap((step): Step[] => {
  if (step.id === 'agent-review') {
    return []
  }
  return step.id === 'testing' ? [{ id: 'review', kind: 'review', gate: 'manual' }, step] : [step]
})

function event(type: EventType, payload: Record<string, unknown> = {}): TaskEvent {
  return { taskId: 'T-1', source: 'system', type, at: '2026-10-08T09:00:00Z', payload }
}

function at(stage: string, state: StagePosition['state'], lanes = {}): StagePosition {
  return { stage, state, lanes }
}

function run(position: StagePosition, ...events: TaskEvent[]) {
  let current = position
  const accepted: boolean[] = []
  for (const next of events) {
    const applied = applyTaskEvent(current, next, PIPELINE)
    current = applied.position
    accepted.push(applied.accepted)
  }
  return { position: current, accepted }
}

describe('deriveTaskState with the legacy pipeline', () => {
  it('starts in draft at implement with no events', () => {
    expect(deriveTaskState([], LEGACY_PIPELINE)).toEqual({
      position: at('implement', 'draft'),
      rejected: [],
    })
  })

  it('waits for review after the agent reports done, then for the release after approval', () => {
    const result = deriveTaskState(
      [
        event('task.created'),
        event('task.readied'),
        event('agent.started'),
        event('agent.asked'),
        event('agent.resumed'),
        event('stage.reported'),
        event('stage.checked'),
      ],
      LEGACY_PIPELINE,
    )
    expect(result).toEqual({ position: at('review', 'review'), rejected: [] })

    const approved = deriveTaskState(
      [
        event('task.readied'),
        event('agent.started'),
        event('stage.checked'),
        event('review.approved'),
      ],
      LEGACY_PIPELINE,
    )
    expect(approved.position).toEqual(at('release', 'ready'))
  })

  it('goes back to implement, to start it again, when changes are requested', () => {
    const result = deriveTaskState(
      [
        event('task.readied'),
        event('agent.started'),
        event('stage.checked'),
        event('review.changes-requested'),
      ],
      LEGACY_PIPELINE,
    )
    expect(result.position).toEqual(at('implement', 'ready'))
  })

  it('moves a failed stage to ready on retry', () => {
    const result = deriveTaskState(
      [event('task.readied'), event('agent.started'), event('agent.failed'), event('task.retried')],
      LEGACY_PIPELINE,
    )
    expect(result.position).toEqual(at('implement', 'ready'))
  })

  it('fails a task that waited for the user when its agent, started again, did not start', () => {
    const result = deriveTaskState(
      [event('task.readied'), event('agent.started'), event('agent.asked'), event('agent.failed')],
      LEGACY_PIPELINE,
    )
    expect(result.position).toEqual(at('implement', 'failed'))
  })

  it('skips an event that does not fit and reports it', () => {
    const early = event('agent.finished')
    const result = deriveTaskState([event('task.readied'), early], LEGACY_PIPELINE)
    expect(result.position.state).toBe('ready')
    expect(result.rejected).toEqual([early])
  })

  it('keeps a terminal state against later events', () => {
    const late = event('agent.started')
    const result = deriveTaskState([event('task.cancelled'), late], LEGACY_PIPELINE)
    expect(result.position.state).toBe('cancelled')
    expect(result.rejected).toEqual([late])
  })

  it('finishes a task in review or release when its pull request is merged, and only then', () => {
    const steps = LEGACY_PIPELINE
    expect(applyTaskEvent(at('review', 'review'), event('pr.merged'), steps)).toEqual({
      position: at('release', 'done'),
      accepted: true,
      moved: true,
    })
    expect(applyTaskEvent(at('implement', 'running'), event('pr.merged'), steps).accepted).toBe(
      false,
    )
  })
})

describe('applyTaskEvent with a manual review', () => {
  it('accepts record-only events in any state without a change', () => {
    const types = [
      'task.created',
      'git.pushed',
      'issue.linked',
      'context.fetched',
      'attachment.added',
      'stage.skip-set',
    ] as const
    for (const type of types) {
      expect(run(at('plan', 'running'), event(type))).toEqual({
        position: at('plan', 'running'),
        accepted: [true],
      })
    }
  })

  it('waits for approval when a manual stage passes its checks, then enters the next step', () => {
    const { position, accepted } = run(
      at('spec', 'ready'),
      event('stage.started'),
      event('stage.reported'),
      event('stage.checked'),
    )
    expect(position).toEqual(at('spec', 'review'))
    expect(accepted).toEqual([true, true, true])

    const approved = applyTaskEvent(position, event('stage.approved'), PIPELINE)
    expect(approved).toEqual({ position: at('plan', 'ready'), accepted: true, moved: true })
  })

  it('enters both stages of a group as ready', () => {
    const { position } = run(at('plan', 'review'), event('stage.approved'))
    expect(position).toEqual(at('build', 'ready', { 'test-spec': 'ready', implement: 'ready' }))
  })

  it('needs the stage name when an event fits more than one stage of a group', () => {
    const group = at('build', 'ready', { 'test-spec': 'ready', implement: 'ready' })
    expect(run(group, event('stage.started')).accepted).toEqual([false])
    expect(run(group, event('stage.started', { stage: 'nope' })).accepted).toEqual([false])
  })

  it('keeps each lane on its own and shows the most urgent one', () => {
    const group = at('build', 'ready', { 'test-spec': 'ready', implement: 'ready' })
    const { position } = run(
      group,
      event('stage.started', { stage: 'test-spec' }),
      event('stage.started', { stage: 'implement' }),
      event('stage.checked', { stage: 'test-spec' }),
    )
    expect(position).toEqual(at('build', 'review', { 'test-spec': 'review', implement: 'running' }))

    const asking = run(position, event('agent.asked', { stage: 'implement' })).position
    expect(asking.state).toBe('needs-input')
  })

  it('enters the next step only when every lane is finished', () => {
    const group = at('build', 'running', { 'test-spec': 'review', implement: 'running' })
    const implemented = run(group, event('stage.checked', { stage: 'implement' })).position
    expect(implemented).toEqual(at('build', 'review', { 'test-spec': 'review', implement: 'done' }))

    const approved = applyTaskEvent(
      implemented,
      event('stage.approved', { stage: 'test-spec' }),
      PIPELINE,
    )
    expect(approved).toEqual({ position: at('review', 'review'), accepted: true, moved: true })
  })

  it('takes a failed check to needs input only when it is final', () => {
    expect(run(at('plan', 'running'), event('stage.check-failed')).position.state).toBe('running')
    expect(
      run(at('plan', 'running'), event('stage.check-failed', { final: true })).position.state,
    ).toBe('needs-input')
  })

  it('lets you approve a stage anyway from needs input', () => {
    expect(run(at('plan', 'needs-input'), event('stage.approved')).position).toEqual(
      at('build', 'ready', { 'test-spec': 'ready', implement: 'ready' }),
    )
  })

  it('sends a task back to one lane of a group and keeps the other result', () => {
    const { position } = run(at('review', 'review'), event('stage.reopened', { to: 'implement' }))
    expect(position).toEqual(at('build', 'ready', { 'test-spec': 'done', implement: 'ready' }))
  })

  it('sends changes asked in review to the code stage of the step before', () => {
    const { position } = run(at('review', 'review'), event('review.changes-requested'))
    expect(position).toEqual(at('build', 'ready', { 'test-spec': 'done', implement: 'ready' }))
  })

  it('sends changes asked about a stage that waits for approval back to that stage', () => {
    expect(run(at('spec', 'review'), event('review.changes-requested')).position).toEqual(
      at('spec', 'ready'),
    )
    const group = at('build', 'review', { 'test-spec': 'review', implement: 'running' })
    expect(run(group, event('review.changes-requested')).position).toEqual(
      at('build', 'running', { 'test-spec': 'ready', implement: 'running' }),
    )
  })

  it('keeps the other stages of the current group when one of them is sent back', () => {
    const group = at('build', 'review', { 'test-spec': 'review', implement: 'running' })
    expect(run(group, event('stage.reopened', { to: 'implement' })).position).toEqual(
      at('build', 'review', { 'test-spec': 'review', implement: 'ready' }),
    )
  })

  it('sends a task back to a stage that waits for a person in its waiting state', () => {
    expect(run(at('testing', 'running'), event('stage.reopened', { to: 'review' }))).toEqual({
      position: at('review', 'review'),
      accepted: [true],
    })
  })

  it('takes an event with no stage name for the only stage of a group it fits', () => {
    const group = at('build', 'review', { 'test-spec': 'review', implement: 'running' })
    expect(run(group, event('review.approved')).position).toEqual(
      at('build', 'running', { 'test-spec': 'done', implement: 'running' }),
    )
    expect(run(group, event('agent.asked')).position.lanes).toEqual({
      'test-spec': 'review',
      implement: 'needs-input',
    })
  })

  it('takes the stage of the terminal an event came from', () => {
    const group = at('build', 'running', { 'test-spec': 'running', implement: 'running' })
    expect(run(group, event('agent.asked')).accepted).toEqual([false])
    expect(run(group, event('agent.asked', { terminal: '2' })).position.lanes).toEqual({
      'test-spec': 'needs-input',
      implement: 'running',
    })
  })

  it('makes a stage whose agent was stopped wait to start again', () => {
    expect(run(at('spec', 'running'), event('agent.stopped')).position).toEqual(at('spec', 'ready'))
    expect(run(at('spec', 'needs-input'), event('agent.stopped')).position).toEqual(
      at('spec', 'ready'),
    )
    expect(run(at('spec', 'review'), event('agent.stopped')).accepted).toEqual([false])
    expect(run(at('spec', 'failed'), event('agent.stopped')).accepted).toEqual([false])
  })

  it('stops only the named stage of a group', () => {
    const group = at('build', 'needs-input', { 'test-spec': 'review', implement: 'needs-input' })
    expect(run(group, event('agent.stopped', { stage: 'implement' })).position).toEqual(
      at('build', 'review', { 'test-spec': 'review', implement: 'ready' }),
    )
  })

  it('does not send a task forward with reopen', () => {
    expect(run(at('spec', 'review'), event('stage.reopened', { to: 'release' })).accepted).toEqual([
      false,
    ])
  })

  it('skips a stage without output', () => {
    expect(run(at('context', 'draft'), event('stage.skipped')).position).toEqual(
      at('spec', 'ready'),
    )
  })

  it('finishes the task after the last stage', () => {
    expect(run(at('release', 'review'), event('stage.approved')).position).toEqual(
      at('release', 'done'),
    )
  })

  it('passes an auto stage when its checks pass', () => {
    expect(run(at('testing', 'running'), event('stage.checked')).position).toEqual(
      at('release', 'ready'),
    )
  })

  it('waits for you when the agent ends its turn without reporting done', () => {
    expect(run(at('plan', 'running'), event('agent.finished')).position).toEqual(
      at('plan', 'needs-input'),
    )
  })

  it('keeps a stage running while the agent waits for its background work', () => {
    expect(run(at('plan', 'running'), event('agent.waiting'))).toEqual({
      position: at('plan', 'running'),
      accepted: [true],
    })
    expect(run(at('plan', 'needs-input'), event('agent.waiting')).accepted).toEqual([false])
  })

  it('fails a stage whose command failed', () => {
    expect(run(at('testing', 'running'), event('stage.failed')).position).toEqual(
      at('testing', 'failed'),
    )
  })

  it('ignores an event about another stage', () => {
    expect(run(at('plan', 'running'), event('stage.checked', { stage: 'spec' })).accepted).toEqual([
      false,
    ])
  })

  it('lets a human override set any state, and move to a stage', () => {
    expect(
      run(at('release', 'cancelled'), event('state.overridden', { to: 'ready' })).position,
    ).toEqual(at('release', 'ready'))
    expect(
      run(at('review', 'review'), event('state.overridden', { to: 'running', stage: 'plan' }))
        .position,
    ).toEqual(at('plan', 'running'))
    expect(run(at('plan', 'ready'), event('state.overridden', { to: 'nope' })).accepted).toEqual([
      false,
    ])
  })
})

describe('completeEvent', () => {
  it('names the stage an event is about when the sender left it out', () => {
    const group = at('build', 'review', { 'test-spec': 'review', implement: 'running' })
    const inGroup = completeEvent(group, event('review.approved'), PIPELINE)
    expect(inGroup.payload).toEqual({ stage: 'test-spec' })
    const alone = completeEvent(at('plan', 'review'), event('review.approved'), PIPELINE)
    expect(alone.payload).toEqual({ stage: 'plan' })
  })

  it('names the stage a send back goes to', () => {
    const sent = completeEvent(
      at('review', 'review'),
      event('review.changes-requested', { reason: 'Fix it' }),
      PIPELINE,
    )
    expect(sent.payload).toEqual({ reason: 'Fix it', to: 'implement' })
  })

  it('leaves an event that names its stage, or fits no stage, as it is', () => {
    const named = event('stage.checked', { stage: 'spec' })
    expect(completeEvent(at('plan', 'running'), named, PIPELINE)).toBe(named)
    const group = at('build', 'running', { 'test-spec': 'running', implement: 'running' })
    const unclear = event('agent.asked')
    expect(completeEvent(group, unclear, PIPELINE)).toBe(unclear)
  })
})

describe('events from the shell of a stage with its own agent', () => {
  // Test spec and Notes start their own agents, so they run in Shells 6 and 7.
  const steps: Step[] = [
    {
      id: 'build',
      parallel: [
        { id: 'implement', kind: 'agent', gate: 'auto' },
        {
          id: 'test-spec',
          kind: 'agent',
          gate: 'manual',
          output: 'test-spec.md',
          agent: 'copilot',
        },
        { id: 'notes', kind: 'agent', gate: 'auto', output: 'notes.md', agent: 'cursor' },
      ],
    },
  ]
  const group = at('build', 'running', {
    implement: 'running',
    'test-spec': 'running',
    notes: 'running',
  })
  const from = (terminal: string) =>
    completeEvent(group, event('agent.asked', { terminal }), steps).payload.stage

  it('names the stage of the shell the event came from', () => {
    expect(from('1')).toBe('implement')
    expect(from('6')).toBe('test-spec')
    expect(from('7')).toBe('notes')
  })

  it('names no stage for a shell no stage uses', () => {
    expect(from('2')).toBeUndefined()
    expect(from('9')).toBeUndefined()
  })

  it('moves only the lane of that shell', () => {
    const asked = applyTaskEvent(group, event('agent.asked', { terminal: '7' }), steps)
    expect(asked.position.lanes).toEqual({
      implement: 'running',
      'test-spec': 'running',
      notes: 'needs-input',
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
