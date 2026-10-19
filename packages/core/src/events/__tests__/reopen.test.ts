import { describe, expect, it } from 'vitest'
import { completeEvent } from '../apply.js'
import { at, event, on, run, SMALL, steps, walk } from './events.fixture.js'

describe('applyTaskEvent with a manual review', () => {
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

  it('does not send a task forward with reopen', () => {
    expect(run(at('spec', 'review'), event('stage.reopened', { to: 'release' })).accepted).toEqual([
      false,
    ])
  })
})

describe('a branch', () => {
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
})
