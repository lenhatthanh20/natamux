import { describe, expect, it } from 'vitest'
import { DEFAULT_PIPELINE } from '../default-pipeline.js'
import {
  combinedState,
  describeCondition,
  enterStep,
  findStep,
  firstPosition,
  isNotTaken,
  pathOf,
  stepIndex,
} from '../position.js'
import { type BranchPath, type Stage, type StageBranch, type Step, stagesOf } from '../schema.js'

describe('positions', () => {
  it('starts a task in draft at its first step', () => {
    expect(firstPosition(DEFAULT_PIPELINE)).toEqual({ stage: 'context', state: 'draft', lanes: {} })
  })

  it('starts a review stage waiting for you, and a group with a state per stage', () => {
    const review: Step = { id: 'review', kind: 'review', gate: 'manual' }
    const build = DEFAULT_PIPELINE[3]
    if (build === undefined) {
      throw new Error('The default pipeline changed')
    }
    expect(enterStep(review)).toEqual({ stage: 'review', state: 'review', lanes: {} })
    expect(enterStep(build)).toEqual({
      stage: 'build',
      state: 'ready',
      lanes: { 'test-spec': 'ready', implement: 'ready' },
    })
  })

  it('shows the most urgent lane of a group', () => {
    expect(combinedState({ a: 'running', b: 'review' })).toBe('review')
    expect(combinedState({ a: 'needs-input', b: 'failed' })).toBe('needs-input')
    expect(combinedState({ a: 'done', b: 'running' })).toBe('running')
    expect(combinedState({ a: 'done', b: 'done' })).toBe('done')
  })

  it('finds the step of a stage inside a group', () => {
    expect(stepIndex(DEFAULT_PIPELINE, 'implement')).toBe(3)
    expect(stepIndex(DEFAULT_PIPELINE, 'build')).toBe(3)
    expect(stepIndex(DEFAULT_PIPELINE, 'nope')).toBe(-1)
  })
})

describe('a branch', () => {
  const plan: Stage = { id: 'plan', kind: 'agent', gate: 'manual', output: 'plan.md' }
  const implement: Stage = { id: 'implement', kind: 'agent', gate: 'auto' }
  const testing: Stage = { id: 'testing', kind: 'testing', gate: 'auto' }
  const review: Stage = { id: 'review', kind: 'review', gate: 'manual' }
  const release: Stage = { id: 'release', kind: 'release', gate: 'manual' }
  const small: BranchPath = {
    id: 'small',
    name: 'Small change',
    when: { type: 'output', file: 'plan.md', section: 'Size', contains: 'small' },
    stages: [implement, testing],
  }
  const prReview: BranchPath = {
    id: 'pr-review',
    name: 'Review PR',
    when: { type: 'command', run: 'test -f .no-review' },
    not: true,
    stages: [review],
  }
  const branch: StageBranch = { id: 'after-plan', name: 'After plan', paths: [small, prReview] }
  const steps: Step[] = [plan, branch, release]

  it('finds the stages of its paths, and the path of a stage', () => {
    expect(stagesOf(branch).map((stage) => stage.id)).toEqual(['implement', 'testing', 'review'])
    expect(findStep(steps, 'review')).toBe(branch)
    expect(stepIndex(steps, 'testing')).toBe(1)
    expect(stepIndex(steps, 'after-plan')).toBe(1)
    expect(pathOf(steps, 'testing')).toEqual({ branch, path: small, index: 1 })
    expect(pathOf(steps, 'review')).toEqual({ branch, path: prReview, index: 0 })
    expect(pathOf(steps, 'plan')).toBeUndefined()
    expect(pathOf(steps, 'nope')).toBeUndefined()
  })

  it('says its condition in words', () => {
    expect(describeCondition(small)).toBe('plan.md "Size" contains "small"')
    const file = { type: 'output', file: 'plan.md', contains: 'small' } as const
    expect(describeCondition({ when: file })).toBe('plan.md contains "small"')
    expect(describeCondition({ when: file, not: true })).toBe('plan.md does not contain "small"')
    expect(describeCondition({ when: { type: 'command', run: 'test -f x' } })).toBe(
      'test -f x succeeds',
    )
    expect(describeCondition(prReview)).toBe('test -f .no-review does not succeed')
    expect(describeCondition({})).toBe('Otherwise')
    expect(describeCondition({ not: true })).toBe('Otherwise')
  })

  it('is entered at the first stage of the path taken', () => {
    expect(enterStep(branch, undefined, 'pr-review')).toEqual({
      stage: 'review',
      state: 'review',
      lanes: {},
    })
    expect(enterStep(branch, 'draft', 'small')).toEqual({
      stage: 'implement',
      state: 'draft',
      lanes: {},
    })
  })

  it('tells the stages of a path the task did not take', () => {
    expect(isNotTaken(steps, { 'after-plan': 'small' }, 'review')).toBe(true)
    expect(isNotTaken(steps, { 'after-plan': 'small' }, 'implement')).toBe(false)
    expect(isNotTaken(steps, { 'after-plan': '' }, 'implement')).toBe(true)
    expect(isNotTaken(steps, { 'after-plan': 'small' }, 'plan')).toBe(false)
    expect(isNotTaken(steps, {}, 'review')).toBe(false)
    expect(isNotTaken(steps, undefined, 'review')).toBe(false)
  })
})
