import { describe, expect, it } from 'vitest'
import { LEGACY_PIPELINE } from '../../stages/default-pipeline.js'
import type { Step } from '../../stages/schema.js'
import { applyTaskEvent } from '../apply.js'
import { deriveTaskState, entersBranch } from '../derive.js'
import { at, branch, event, on, plan, SMALL, steps } from './events.fixture.js'

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

describe('a branch', () => {
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
