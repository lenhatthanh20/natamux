import { DEFAULT_PIPELINE, type Stage, stagesOf } from '@natamux/core'
import { describe, expect, it } from 'vitest'
import { linkKind } from '../components/StageMarker'
import { finishedWhen, formatDuration, stageCount, stepTime } from './stage-view'

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
