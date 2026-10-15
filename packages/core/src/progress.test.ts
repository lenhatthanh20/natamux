import { describe, expect, it } from 'vitest'
import type { EventType, TaskEvent } from './events.js'
import { stageProgress } from './progress.js'
import { DEFAULT_PIPELINE, type Step } from './stage.js'
import type { Task } from './task.js'

let minute = 0
function event(
  type: EventType,
  payload: Record<string, unknown> = {},
  source: TaskEvent['source'] = 'system',
): TaskEvent {
  minute += 1
  return {
    taskId: 'T-1',
    source,
    type,
    at: `2026-10-09T09:${String(minute).padStart(2, '0')}:00.000Z`,
    payload,
  }
}

describe('stageProgress', () => {
  it('marks stages done, current and upcoming, with times, rounds and evidence', () => {
    minute = 0
    const events = [
      event('stage.started', { stage: 'context' }),
      event('stage.checked', { stage: 'context', passes: ['context.md was written'] }),
      event('stage.approved', { stage: 'context' }, 'human'),
      event('stage.started', { stage: 'spec' }),
      event('stage.check-failed', { stage: 'spec', reasons: ['spec.md is missing'] }),
    ]
    const progress = stageProgress(
      { stage: 'spec', state: 'running', lanes: {} },
      events,
      DEFAULT_PIPELINE,
    )

    expect(progress.map((stage) => [stage.stage, stage.status])).toEqual([
      ['context', 'done'],
      ['spec', 'current'],
      ['plan', 'upcoming'],
      ['test-spec', 'upcoming'],
      ['implement', 'upcoming'],
      ['testing', 'upcoming'],
      ['agent-review', 'upcoming'],
      ['release', 'upcoming'],
    ])
    expect(progress[0]).toMatchObject({
      startedAt: '2026-10-09T09:01:00.000Z',
      endedAt: '2026-10-09T09:03:00.000Z',
      approvedBy: 'human',
      passes: ['context.md was written'],
      rounds: 1,
    })
    expect(progress[1]).toMatchObject({ state: 'running', reasons: ['spec.md is missing'] })
    expect(progress[3]).toMatchObject({ group: 'build', name: 'Test spec' })
  })

  it('shows each stage of a group on its own, and counts a stage sent back', () => {
    minute = 0
    const events = [
      event('stage.started', { stage: 'implement' }),
      event('stage.reopened', { to: 'implement', reason: 'Add the error cases' }, 'human'),
      event('stage.started', { stage: 'implement' }),
    ]
    const progress = stageProgress(
      { stage: 'build', state: 'review', lanes: { 'test-spec': 'review', implement: 'done' } },
      events,
      DEFAULT_PIPELINE,
    )
    const testSpec = progress.find((stage) => stage.stage === 'test-spec')
    const implement = progress.find((stage) => stage.stage === 'implement')
    expect(testSpec).toMatchObject({ status: 'current', state: 'review' })
    expect(implement).toMatchObject({
      status: 'done',
      rounds: 2,
      sentBackFor: 'Add the error cases',
    })
  })

  it('reads an approval from review and a request for changes like their stage events', () => {
    minute = 0
    const events = [
      event('stage.started', { stage: 'plan' }),
      event('stage.checked', { stage: 'plan' }),
      event('review.approved', { stage: 'plan' }, 'human'),
      event('stage.started', { stage: 'implement' }),
      event('review.changes-requested', { to: 'implement', reason: 'Fix the dates' }, 'human'),
      event('stage.started', { stage: 'implement' }),
    ]
    const progress = stageProgress(
      { stage: 'build', state: 'running', lanes: { 'test-spec': 'review', implement: 'running' } },
      events,
      DEFAULT_PIPELINE,
    )
    const plan = progress.find((stage) => stage.stage === 'plan')
    const testSpec = progress.find((stage) => stage.stage === 'test-spec')
    const implement = progress.find((stage) => stage.stage === 'implement')
    expect(plan).toMatchObject({
      status: 'done',
      endedAt: '2026-10-09T09:03:00.000Z',
      approvedBy: 'human',
    })
    expect(testSpec?.startedAt).toBe('2026-10-09T09:05:00.000Z')
    expect(implement).toMatchObject({ rounds: 2, sentBackFor: 'Fix the dates' })
  })

  it('tells a send back by CI from one by a review', () => {
    minute = 0
    const task: Pick<Task, 'stage' | 'state' | 'lanes'> = {
      stage: 'build',
      state: 'running',
      lanes: { 'test-spec': 'review', implement: 'running' },
    }
    const byCi = [
      event('stage.started', { stage: 'implement' }),
      event('stage.reopened', {
        to: 'implement',
        from: 'implement',
        reason: 'CI failed on Implement:',
        ci: true,
      }),
      event('stage.started', { stage: 'implement' }),
    ]
    const implementOf = (events: TaskEvent[]) =>
      stageProgress(task, events, DEFAULT_PIPELINE).find((stage) => stage.stage === 'implement')

    expect(implementOf(byCi)).toMatchObject({ rounds: 2, sentBackByCi: true })

    const byReview = [
      ...byCi,
      event('review.changes-requested', { to: 'implement', reason: 'Fix the dates' }, 'human'),
      event('stage.started', { stage: 'implement' }),
    ]
    expect(implementOf(byReview)).toMatchObject({ rounds: 3, sentBackByCi: false })

    const reviewThenCi = [
      ...byReview,
      event('stage.reopened', { to: 'implement', from: 'implement', reason: 'CI', ci: true }),
      event('stage.started', { stage: 'implement' }),
    ]
    expect(implementOf(reviewThenCi)).toMatchObject({ rounds: 4, sentBackByCi: true })

    expect(implementOf(byCi.slice(0, 1))).toMatchObject({ sentBackFor: null, sentBackByCi: false })
    const old = [
      event('stage.started', { stage: 'implement' }),
      event('stage.reopened', { to: 'implement', reason: 'Before CI send backs' }),
      event('stage.started', { stage: 'implement' }),
    ]
    expect(implementOf(old)).toMatchObject({ rounds: 2, sentBackByCi: false })
  })

  it('marks a skipped stage, and every stage of a done task', () => {
    minute = 0
    const events = [event('stage.skipped', { stage: 'context' })]
    const progress = stageProgress(
      { stage: 'spec', state: 'ready', lanes: {} },
      events,
      DEFAULT_PIPELINE,
    )
    expect(progress[0]?.status).toBe('skipped')

    const done = stageProgress({ stage: 'release', state: 'done', lanes: {} }, [], DEFAULT_PIPELINE)
    expect(done.every((stage) => stage.status === 'done')).toBe(true)
  })

  it('starts a stage that waits for a person when the step before it ended', () => {
    minute = 0
    const events = [
      event('stage.started', { stage: 'testing' }),
      event('stage.checked', { stage: 'testing' }),
    ]
    const steps = [
      { id: 'testing', kind: 'testing', gate: 'auto' },
      { id: 'release', kind: 'release', gate: 'manual' },
    ] as const
    const [, release] = stageProgress({ stage: 'release', state: 'ready', lanes: {} }, events, [
      ...steps,
    ])
    expect(release).toMatchObject({ status: 'current', startedAt: '2026-10-09T09:02:00.000Z' })
  })

  describe('with a branch', () => {
    const steps: Step[] = [
      { id: 'plan', kind: 'agent', gate: 'manual', output: 'plan.md' },
      {
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
          { id: 'pr-review', stages: [{ id: 'review', kind: 'review', gate: 'manual' }] },
        ],
      },
      { id: 'release', kind: 'release', gate: 'manual' },
    ]
    const because = 'plan.md "Size" contains "small"'
    type Branched = Pick<Task, 'stage' | 'state' | 'lanes' | 'paths'>

    it('marks the stages of a path not taken, and gives each stage its path and reason', () => {
      minute = 0
      const events = [
        event('stage.started', { stage: 'plan' }),
        event('stage.checked', { stage: 'plan' }),
        event('stage.approved', { stage: 'plan', path: 'small', because }, 'human'),
        event('stage.started', { stage: 'implement' }),
        event('stage.checked', { stage: 'implement' }),
      ]
      const task: Branched = {
        stage: 'testing',
        state: 'ready',
        lanes: {},
        paths: { 'after-plan': 'small' },
      }
      const progress = stageProgress(task, events, steps)
      expect(progress.map((stage) => [stage.stage, stage.status, stage.path])).toEqual([
        ['plan', 'done', null],
        ['implement', 'done', 'small'],
        ['testing', 'current', 'small'],
        ['review', 'not-taken', 'pr-review'],
        ['release', 'upcoming', null],
      ])
      expect(progress.map((stage) => stage.because)).toEqual([null, because, null, null, null])
      expect(progress[2]).toMatchObject({ startedAt: '2026-10-09T09:05:00.000Z', rounds: 1 })
      expect(progress[3]).toMatchObject({ startedAt: null, endedAt: null, rounds: 0 })
    })

    it('starts the first stage of the path taken when the event that chose it came', () => {
      minute = 0
      const events = [
        event('stage.started', { stage: 'plan' }),
        event('stage.approved', { stage: 'plan', path: 'pr-review', because: 'Otherwise' }),
      ]
      const task: Branched = {
        stage: 'review',
        state: 'review',
        lanes: {},
        paths: { 'after-plan': 'pr-review' },
      }
      const progress = stageProgress(task, events, steps)
      expect(progress.find((stage) => stage.stage === 'review')).toMatchObject({
        status: 'current',
        startedAt: '2026-10-09T09:02:00.000Z',
        because: 'Otherwise',
      })
      expect(progress.filter((stage) => stage.status === 'not-taken').length).toBe(2)
    })

    it('shows every stage of a skipped branch as not taken, and all as upcoming before it', () => {
      minute = 0
      const events = [event('stage.approved', { stage: 'plan', because: 'No condition held' })]
      for (const state of ['ready', 'done'] as const) {
        const task = { stage: 'release', state, lanes: {}, paths: { 'after-plan': '' } }
        const progress = stageProgress(task, events, steps)
        expect(progress.map((stage) => stage.status)).toEqual([
          'done',
          'not-taken',
          'not-taken',
          'not-taken',
          state === 'done' ? 'done' : 'current',
        ])
        expect(progress.every((stage) => stage.because === null)).toBe(true)
      }
      const choices: (Record<string, string> | undefined)[] = [{}, undefined]
      for (const paths of choices) {
        const task = { stage: 'plan', state: 'running' as const, lanes: {}, paths }
        const before = stageProgress(task, [], steps)
        expect(before.slice(1).map((stage) => stage.status)).toEqual([
          'upcoming',
          'upcoming',
          'upcoming',
          'upcoming',
        ])
      }
    })
  })
})
