import { describe, expect, it } from 'vitest'
import {
  DEFAULT_PIPELINE,
  LEGACY_STEP_REFS,
  resolveSteps,
  stepRefSchema,
  stepRefsOf,
} from '../default-pipeline.js'
import { activeSteps, type BranchPath, type Stage, type StageBranch, type Step } from '../schema.js'

describe('the default pipeline', () => {
  it('runs the automated tests, then an agent review, and waits for no person', () => {
    expect(stepRefsOf(DEFAULT_PIPELINE)).toEqual([
      'context',
      'spec',
      'plan',
      { id: 'build', parallel: ['test-spec', 'implement'] },
      'testing',
      'agent-review',
      'release',
    ])
    expect(DEFAULT_PIPELINE[1]).toEqual({
      id: 'spec',
      kind: 'agent',
      gate: 'manual',
      output: 'spec.md',
      sections: ['Goal', 'Scope', 'Out of scope', 'Acceptance criteria', 'Code map'],
    })
    expect(activeSteps(DEFAULT_PIPELINE)).toEqual(DEFAULT_PIPELINE)
  })
})

describe('resolveSteps', () => {
  it('turns the step ids of a task back into steps with their settings', () => {
    expect(resolveSteps(stepRefsOf(DEFAULT_PIPELINE), DEFAULT_PIPELINE)).toEqual(DEFAULT_PIPELINE)
  })

  it('finds a stage of a group for a task that has it on its own', () => {
    const [implement] = resolveSteps(LEGACY_STEP_REFS, DEFAULT_PIPELINE)
    expect(implement).toEqual({
      id: 'implement',
      kind: 'agent',
      gate: 'auto',
      workflow: true,
      ci: true,
    })
  })

  it('makes a stage of its own that is gone from the settings a manual one', () => {
    expect(resolveSteps(['sign-off'], DEFAULT_PIPELINE)).toEqual([
      { id: 'sign-off', kind: 'manual', gate: 'manual' },
    ])
  })

  it('keeps the catalog settings of a catalog stage that is gone from the settings', () => {
    const [security] = resolveSteps(['security'], DEFAULT_PIPELINE)
    expect(security).toMatchObject({ id: 'security', kind: 'agent', output: 'security.md' })
  })

  it('still runs a stage that is off for a task that has it', () => {
    const pipeline: Step[] = [{ id: 'plan', kind: 'agent', gate: 'auto', enabled: false }]
    const [plan] = resolveSteps(['plan'], pipeline)
    expect(plan).toMatchObject({ id: 'plan', gate: 'auto', enabled: false })
  })

  it('keeps the default settings of a default stage that is gone from the settings', () => {
    const [plan] = resolveSteps(['plan'], [{ id: 'implement', kind: 'agent', gate: 'auto' }])
    expect(plan).toMatchObject({ id: 'plan', kind: 'agent', output: 'plan.md' })
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

  it('keeps the ids of its paths in a task, and reads the rest from the settings', () => {
    const refs = stepRefsOf(steps)
    expect(refs).toEqual([
      'plan',
      {
        id: 'after-plan',
        paths: [
          { id: 'small', stages: ['implement', 'testing'] },
          { id: 'pr-review', stages: ['review'] },
        ],
      },
      'release',
    ])
    expect(refs.every((ref) => stepRefSchema.safeParse(ref).success)).toBe(true)
    expect(
      stepRefSchema.safeParse({ id: 'b', paths: [{ id: 'a', stages: ['implement'] }] }).success,
    ).toBe(false)
    expect(resolveSteps(refs, steps)).toEqual(steps)

    // The settings changed: a new condition counts, and a path that is gone keeps its stages, with
    // the catalog settings of a stage that is gone too.
    const now: Step[] = [
      plan,
      {
        id: 'after-plan',
        paths: [
          { ...small, not: true },
          { id: 'other', stages: [testing] },
        ],
      },
      release,
    ]
    expect(resolveSteps(refs, now)[1]).toEqual({
      id: 'after-plan',
      paths: [
        { ...small, not: true },
        { id: 'pr-review', stages: [{ ...review, name: 'Manual review' }] },
      ],
    })
  })
})
