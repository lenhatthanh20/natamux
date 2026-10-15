import { describe, expect, it } from 'vitest'
import {
  applyTaskEvent,
  completeEvent,
  deriveTaskState,
  type EventType,
  entersBranch,
  GATE_EVENT_TYPES,
  type TaskEvent,
  taskEventSchema,
} from './events.js'
import {
  DEFAULT_PIPELINE,
  LEGACY_PIPELINE,
  type Stage,
  type StageBranch,
  type StagePosition,
  type Step,
} from './stage.js'

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

  it('runs a failed stage again when its agent is resumed', () => {
    const result = deriveTaskState(
      [
        event('task.readied'),
        event('agent.started'),
        event('agent.failed'),
        event('agent.resumed'),
      ],
      LEGACY_PIPELINE,
    )
    expect(result.position).toEqual(at('implement', 'running'))
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

  it('takes a failed agent stage up again in its own lane, then moves on', () => {
    const group = at('build', 'failed', { 'test-spec': 'done', implement: 'failed' })
    const resumed = run(group, event('agent.resumed', { stage: 'implement' })).position
    expect(resumed).toEqual(at('build', 'running', { 'test-spec': 'done', implement: 'running' }))

    const checked = applyTaskEvent(
      resumed,
      event('stage.checked', { stage: 'implement' }),
      PIPELINE,
    )
    expect(checked).toEqual({ position: at('review', 'review'), accepted: true, moved: true })
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

describe('a branch', () => {
  const plan: Stage = { id: 'plan', kind: 'agent', gate: 'manual', output: 'plan.md' }
  const branch: StageBranch = {
    id: 'after-plan',
    paths: [
      {
        id: 'small',
        when: { type: 'output', file: 'plan.md', section: 'Size', contains: 'small' },
        stages: [
          { id: 'implement', kind: 'agent', gate: 'auto' },
          { id: 'testing', kind: 'testing', gate: 'auto' },
        ],
      },
      {
        id: 'pr-review',
        when: { type: 'command', run: 'test -f .no-review' },
        not: true,
        stages: [{ id: 'review', kind: 'review', gate: 'manual' }],
      },
    ],
  }
  const steps: Step[] = [plan, branch, { id: 'release', kind: 'release', gate: 'manual' }]
  const SMALL = { 'after-plan': 'small' }

  function on(stage: string, state: StagePosition['state'], paths: Record<string, string>) {
    return { stage, state, lanes: {}, paths }
  }

  function walk(position: StagePosition, ...events: TaskEvent[]) {
    let current = position
    const accepted: boolean[] = []
    for (const next of events) {
      const applied = applyTaskEvent(current, next, steps)
      current = applied.position
      accepted.push(applied.accepted)
    }
    return { position: current, accepted }
  }

  it('enters the first stage of the path the event names, and records it', () => {
    const approved = applyTaskEvent(
      on('plan', 'review', {}),
      event('stage.approved', { stage: 'plan', path: 'small', because: 'plan.md has it' }),
      steps,
    )
    expect(approved).toEqual({
      position: on('implement', 'ready', SMALL),
      accepted: true,
      moved: true,
    })
    expect(
      walk(on('plan', 'review', {}), event('stage.approved', { path: 'pr-review' })).position,
    ).toEqual(on('review', 'review', { 'after-plan': 'pr-review' }))
  })

  it('skips the branch when the event names no path, or a path the task does not have', () => {
    const skipped = on('release', 'ready', { 'after-plan': '' })
    expect(applyTaskEvent(on('plan', 'review', {}), event('stage.approved'), steps)).toEqual({
      position: skipped,
      accepted: true,
      moved: true,
    })
    expect(
      walk(on('plan', 'review', {}), event('stage.approved', { path: 'gone' })).position,
    ).toEqual(skipped)
    // A position from before branches existed has no record of the choices yet.
    expect(walk(at('plan', 'review'), event('stage.approved')).position).toEqual(skipped)
  })

  it('takes the path of an event only at the first branch it reaches', () => {
    const next: StageBranch = {
      id: 'after-build',
      paths: [
        {
          id: 'docs',
          when: { type: 'command', run: 'true' },
          stages: [{ id: 'write-docs', kind: 'agent', gate: 'auto' }],
        },
        { id: 'none', stages: [{ id: 'qa', kind: 'manual', gate: 'manual' }] },
      ],
    }
    const two: Step[] = [plan, branch, next]
    const approved = event('stage.approved', { path: 'docs' })
    const applied = applyTaskEvent(on('plan', 'review', {}), approved, two)
    expect(applied.position).toEqual({
      stage: 'plan',
      state: 'done',
      lanes: {},
      paths: { 'after-plan': '', 'after-build': '' },
    })
  })

  it('goes through the stages of the path, then to the step after the branch', () => {
    const { position, accepted } = walk(
      on('implement', 'running', SMALL),
      event('stage.checked'),
      event('stage.started'),
      event('agent.asked'),
      event('agent.resumed'),
      event('stage.checked'),
    )
    expect(accepted).toEqual([true, true, true, true, true])
    expect(position).toEqual(on('release', 'ready', SMALL))
    expect(walk(on('implement', 'ready', SMALL), event('stage.skipped')).position).toEqual(
      on('testing', 'ready', SMALL),
    )
    const prReview = { 'after-plan': 'pr-review' }
    expect(walk(on('review', 'review', prReview), event('review.approved'))).toEqual({
      position: on('release', 'ready', prReview),
      accepted: [true],
    })
  })

  it('names the stage in a path an event is about, and keeps the path and the reason', () => {
    const checked = completeEvent(on('implement', 'running', SMALL), event('stage.checked'), steps)
    expect(checked.payload).toEqual({ stage: 'implement' })
    const chosen = event('stage.approved', { path: 'small', because: 'Otherwise' })
    expect(completeEvent(on('plan', 'review', {}), chosen, steps).payload).toEqual({
      path: 'small',
      because: 'Otherwise',
      stage: 'plan',
    })
  })

  it('sends a task back only to a stage on the way it went', () => {
    const atTesting = on('testing', 'running', SMALL)
    expect(walk(atTesting, event('stage.reopened', { to: 'review' }))).toEqual({
      position: atTesting,
      accepted: [false],
    })
    expect(walk(atTesting, event('stage.reopened', { to: 'implement' })).position).toEqual(
      on('implement', 'ready', SMALL),
    )
    const atImplement = on('implement', 'running', SMALL)
    expect(walk(atImplement, event('stage.reopened', { to: 'testing' })).accepted).toEqual([false])
    const skipped = on('release', 'review', { 'after-plan': '' })
    expect(walk(skipped, event('stage.reopened', { to: 'implement' })).accepted).toEqual([false])
  })

  it('clears the choice when the task goes back to a stage before the branch', () => {
    expect(
      walk(on('implement', 'running', SMALL), event('stage.reopened', { to: 'plan' })).position,
    ).toEqual(on('plan', 'ready', {}))
    expect(
      walk(
        on('implement', 'running', SMALL),
        event('state.overridden', { to: 'review', stage: 'plan' }),
      ).position,
    ).toEqual(on('plan', 'review', {}))
  })

  it('sends a task back without a stage named along the path it took', () => {
    const back = (paths: Record<string, string>) =>
      walk(on('release', 'review', paths), event('stage.reopened')).position
    expect(back(SMALL)).toEqual(on('implement', 'ready', SMALL))
    expect(back({ 'after-plan': 'pr-review' })).toEqual(on('plan', 'ready', {}))
    expect(back({ 'after-plan': '' })).toEqual(on('plan', 'ready', {}))
    const atReview = on('review', 'review', { 'after-plan': 'pr-review' })
    expect(walk(atReview, event('review.changes-requested')).position).toEqual(
      on('plan', 'ready', {}),
    )
    expect(
      completeEvent(on('release', 'review', SMALL), event('stage.reopened'), steps).payload,
    ).toEqual({ to: 'implement' })
  })

  it('finishes a task on a path when its pull request is merged', () => {
    const atReview = on('review', 'review', { 'after-plan': 'pr-review' })
    expect(applyTaskEvent(atReview, event('pr.merged'), steps)).toEqual({
      position: on('release', 'done', { 'after-plan': 'pr-review' }),
      accepted: true,
      moved: true,
    })
  })

  it('finishes a task on the last stage it took when the pipeline ends in a branch', () => {
    const atReview = on('review', 'review', { 'after-plan': 'pr-review' })
    expect(applyTaskEvent(atReview, event('pr.merged'), [plan, branch])).toEqual({
      position: on('review', 'done', { 'after-plan': 'pr-review' }),
      accepted: true,
      moved: false,
    })
  })

  it('refuses a merge at a stage of a path before its review stage', () => {
    const reviewed: StageBranch = {
      id: 'after-plan',
      paths: [
        {
          id: 'full',
          stages: [
            { id: 'implement', kind: 'agent', gate: 'auto' },
            { id: 'review', kind: 'review', gate: 'manual' },
          ],
        },
      ],
    }
    const full: Step[] = [plan, reviewed]
    const merge = (stage: string) =>
      applyTaskEvent(on(stage, 'running', { 'after-plan': 'full' }), event('pr.merged'), full)
    expect(merge('implement').accepted).toBe(false)
    expect(merge('review').accepted).toBe(true)
  })

  it('gives the same position, with the choices, when its log is replayed', () => {
    const log = [
      event('task.readied'),
      event('stage.started', { stage: 'plan' }),
      event('stage.checked', { stage: 'plan' }),
      event('stage.approved', { stage: 'plan', path: 'small', because: 'plan.md has it' }),
      event('stage.started', { stage: 'implement' }),
      event('stage.checked', { stage: 'implement' }),
    ]
    expect(deriveTaskState(log, steps)).toEqual({
      position: on('testing', 'ready', SMALL),
      rejected: [],
    })
    const again = [
      ...log,
      event('stage.reopened', { to: 'plan' }),
      event('stage.started', { stage: 'plan' }),
      event('stage.checked', { stage: 'plan' }),
      event('stage.approved', { stage: 'plan' }),
    ]
    expect(deriveTaskState(again, steps)).toEqual({
      position: on('release', 'ready', { 'after-plan': '' }),
      rejected: [],
    })
  })

  it('tells when an event would enter a branch, without a path', () => {
    const approve = event('stage.approved')
    expect(entersBranch(on('plan', 'review', {}), approve, steps)).toBe(branch)
    expect(entersBranch(at('plan', 'review'), approve, steps)).toBe(branch)
    expect(
      entersBranch(on('plan', 'review', {}), event('stage.approved', { path: 'small' }), steps),
    ).toBe(branch)
    expect(entersBranch(on('plan', 'running', {}), event('stage.checked'), steps)).toBeUndefined()
    const auto: Step[] = [{ ...plan, gate: 'auto' }, ...steps.slice(1)]
    expect(entersBranch(on('plan', 'running', {}), event('stage.checked'), auto)).toBe(branch)
    const atImplement = on('implement', 'running', SMALL)
    expect(entersBranch(atImplement, event('stage.checked'), steps)).toBeUndefined()
    const atTesting = on('testing', 'running', SMALL)
    expect(entersBranch(atTesting, event('stage.checked'), steps)).toBeUndefined()
    expect(entersBranch(atTesting, event('stage.reopened', { to: 'plan' }), steps)).toBeUndefined()
  })
})
