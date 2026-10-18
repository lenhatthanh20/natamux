import {
  DEFAULT_PIPELINE,
  type Stage,
  type StageBranch,
  type StageProgress,
  type StageStatus,
  stagesOf,
  type TaskEvent,
} from '@natamux/core'
import { describe, expect, it } from 'vitest'
import { linkKind } from '../../components/StageMarker'
import {
  branchStatus,
  finishedWhen,
  formatDuration,
  missingSkills,
  pathLabel,
  ranWithWorkflow,
  sameLane,
  stageCount,
  stepTime,
  takenPath,
  waitingForCi,
} from '../stage-view'

describe('formatDuration', () => {
  it.each([
    ['2026-10-09T09:00:00Z', '2026-10-09T09:00:40Z', '<1m'],
    ['2026-10-09T09:00:00Z', '2026-10-09T09:04:00Z', '4m'],
    ['2026-10-09T09:00:00Z', '2026-10-09T10:05:00Z', '1h 5m'],
    ['2026-10-09T09:00:00Z', '2026-10-09T11:00:00Z', '2h'],
    ['2026-10-09T09:00:00Z', '2026-10-11T12:00:00Z', '2d 3h'],
  ])('says %s to %s as %s', (from, to, text) => {
    expect(formatDuration(from, to)).toBe(text)
  })
})

describe('finishedWhen', () => {
  it('says what finishes a stage from its checks', () => {
    const plan = DEFAULT_PIPELINE[2] as Stage
    expect(finishedWhen(plan)).toBe(
      'Finished when plan.md is written, it has Steps, Files, Tests, and you approve it.',
    )
    expect(finishedWhen({ id: 'release', kind: 'release', gate: 'manual' })).toBe(
      'Finished when the pull request is merged.',
    )
    expect(
      finishedWhen({ id: 'x', kind: 'manual', gate: 'manual', doneWhen: 'Legal signs off.' }),
    ).toBe('Legal signs off.')
  })

  it('names a commit only when the stage needs one', () => {
    const implement = DEFAULT_PIPELINE.flatMap(stagesOf).find((stage) => stage.id === 'implement')
    if (implement === undefined) {
      throw new Error('The default pipeline has no implement stage')
    }
    expect(finishedWhen(implement)).not.toContain('committed')
    expect(finishedWhen({ ...implement, requireCommit: true })).toContain('the work is committed')
  })
})

describe('linkKind', () => {
  const done = { status: 'done' as const, state: null }
  const running = { status: 'current' as const, state: 'running' as const }
  const upcoming = { status: 'upcoming' as const, state: null }
  it('is green behind the work, moving into a running stage, dashed ahead', () => {
    expect(linkKind(done, done)).toBe('done')
    expect(linkKind(done, running)).toBe('flow')
    expect(linkKind(running, upcoming)).toBe('upcoming')
    expect(linkKind(undefined, done)).toBe('hidden')
  })
})

describe('stageCount', () => {
  it('counts done and skipped stages as finished', () => {
    const entry = (status: 'done' | 'skipped' | 'current') => ({ status }) as never
    expect(stageCount([entry('done'), entry('skipped'), entry('current')])).toEqual({
      done: 2,
      total: 3,
    })
  })
})

describe('stageCount with a branch', () => {
  it('leaves out the stages of a path not taken', () => {
    const entry = (status: StageStatus) => ({ status }) as never
    expect(
      stageCount([entry('done'), entry('not-taken'), entry('not-taken'), entry('current')]),
    ).toEqual({ done: 1, total: 2 })
  })
})

describe('the branch of a task', () => {
  const stage = (id: string): Stage => ({ id, kind: 'agent', gate: 'auto' }) as Stage
  const branch: StageBranch = {
    id: 'after-plan',
    paths: [
      { id: 'small', name: 'Small change', stages: [stage('implement'), stage('testing')] },
      { id: 'big', stages: [stage('review')] },
    ],
  }
  const entries = (statuses: Record<string, StageStatus>) =>
    new Map(
      Object.entries(statuses).map(([id, status]) => [id, { stage: id, status }] as const),
    ) as unknown as ReadonlyMap<string, StageProgress>

  it('finds the path the task took by its stages that are not waiting', () => {
    const taken = takenPath(
      branch,
      entries({ implement: 'current', testing: 'upcoming', review: 'not-taken' }),
    )
    expect(taken?.id).toBe('small')
  })

  it('finds no path before the branch is reached, and none when it was skipped', () => {
    const before = entries({ implement: 'upcoming', testing: 'upcoming', review: 'upcoming' })
    const skipped = entries({ implement: 'not-taken', testing: 'not-taken', review: 'not-taken' })
    expect(takenPath(branch, before)).toBeUndefined()
    expect(takenPath(branch, skipped)).toBeUndefined()
  })

  it('gives the status of the diamond', () => {
    const status = (statuses: Record<string, StageStatus>) =>
      branchStatus(branch, entries(statuses))
    expect(status({ implement: 'upcoming', testing: 'upcoming', review: 'upcoming' })).toBe(
      'upcoming',
    )
    expect(status({ implement: 'done', testing: 'current', review: 'not-taken' })).toBe('current')
    expect(status({ implement: 'done', testing: 'done', review: 'not-taken' })).toBe('done')
    expect(status({ implement: 'not-taken', testing: 'not-taken', review: 'not-taken' })).toBe(
      'not-taken',
    )
  })

  it('names a path by its name, or its id', () => {
    expect(pathLabel({ id: 'small', name: 'Small change' })).toBe('Small change')
    expect(pathLabel({ id: 'big' })).toBe('big')
  })

  it('draws no line from a stage of one path to a stage of another', () => {
    const at = (path: string | null) => ({ path })
    expect(sameLane(at(null), at('small'))).toBe(true)
    expect(sameLane(at('small'), at(null))).toBe(true)
    expect(sameLane(at('small'), at('small'))).toBe(true)
    expect(sameLane(at('small'), at('big'))).toBe(false)
    expect(sameLane(undefined, at('big'))).toBe(true)
  })
})

describe('stepTime', () => {
  const now = new Date('2026-10-09T10:00:00Z')
  const lane = (startedAt: string | null, endedAt: string | null, status = 'done') =>
    ({ startedAt, endedAt, status }) as never
  it('spans from the first start to the last end of side by side stages', () => {
    const a = lane('2026-10-09T09:00:00Z', '2026-10-09T09:10:00Z')
    const b = lane('2026-10-09T09:02:00Z', '2026-10-09T09:30:00Z')
    expect(stepTime([a, b], now)).toBe('30m')
  })
  it('counts up to now while a stage runs', () => {
    const a = lane('2026-10-09T09:00:00Z', null, 'current')
    expect(stepTime([a], now)).toBe('1h so far')
  })
  it('is null before a start, or when a finished stage has no end', () => {
    expect(stepTime([lane(null, null, 'upcoming')], now)).toBeNull()
    expect(stepTime([lane('2026-10-09T09:00:00Z', null)], now)).toBeNull()
  })
})

describe('missingSkills', () => {
  const started = (stage: string, payload: Record<string, unknown> = {}): TaskEvent => ({
    taskId: 'T-0001',
    source: 'system',
    type: 'stage.started',
    at: '2026-10-10T08:00:00.000Z',
    payload: { stage, ...payload },
  })

  it('keeps the skills of the latest start of each stage', () => {
    const missing = missingSkills([
      started('spec', { missingSkills: ['a', 'b'], agent: 'codex' }),
      started('plan', { missingSkills: ['c'], agent: 'codex' }),
      started('spec'),
    ])
    expect(missing.get('spec')).toEqual({ names: [], agent: null })
    expect(missing.get('plan')).toEqual({ names: ['c'], agent: 'codex' })
  })
})

describe('waitingForCi', () => {
  const event = (type: TaskEvent['type'], stage: string, extra = {}): TaskEvent => ({
    taskId: 'T-1',
    source: 'system',
    type,
    at: '2026-10-09T09:00:00.000Z',
    payload: { stage, ...extra },
  })

  it('names the stages whose CI check runs now, with their CI branch', () => {
    const waits = waitingForCi([
      event('stage.ci-started', 'implement', { branch: 'natamux-ci/a' }),
      event('stage.checked', 'implement'),
      event('stage.ci-started', 'testing', { branch: 'natamux-ci/a' }),
      event('stage.ci-started', 'docs', { branch: 'natamux-ci/a' }),
      event('stage.check-failed', 'docs'),
    ])
    expect([...waits]).toEqual([['testing', 'natamux-ci/a']])
  })
})

describe('ranWithWorkflow', () => {
  const started = (stage: string, payload: Record<string, unknown> = {}): TaskEvent => ({
    taskId: 'T-1',
    source: 'system',
    type: 'stage.started',
    at: '2026-10-09T09:00:00.000Z',
    payload: { stage, ...payload },
  })

  it('names the stages whose last start asked for a workflow', () => {
    const events = [
      started('plan', { workflow: true }),
      started('implement', { workflow: true }),
      started('implement'),
      started('test-spec'),
    ]
    expect([...ranWithWorkflow(events)]).toEqual(['plan'])
  })
})
