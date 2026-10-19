import { describe, expect, it } from 'vitest'
import { stepRefsOf } from '../default-pipeline.js'
import { pipelineSchema } from '../pipeline-schema.js'
import { activeSteps, type BranchPath, type Stage, type StageBranch } from '../schema.js'

describe('stages that are off', () => {
  const stage = (id: string, kind: string, enabled?: boolean) => ({
    id,
    kind,
    gate: 'manual',
    ...(enabled === undefined ? {} : { enabled }),
  })
  const valid = (steps: unknown[]) => pipelineSchema.safeParse(steps).success

  it('leaves them out of the rules for the context and the release', () => {
    expect(valid([stage('check', 'manual', false), stage('context', 'context')])).toBe(true)
    expect(valid([stage('ship', 'release'), stage('check', 'manual', false)])).toBe(true)
    expect(valid([stage('context', 'context'), stage('more', 'context', false)])).toBe(true)
  })

  it('still needs unique ids, and one stage that is on', () => {
    expect(valid([stage('check', 'manual'), stage('check', 'manual', false)])).toBe(false)
    expect(valid([stage('check', 'manual', false)])).toBe(false)
  })

  it('needs two stages that are on in a group', () => {
    const group = (parallel: unknown[]) => [stage('first', 'manual'), { id: 'build', parallel }]
    expect(valid(group([stage('a', 'agent'), stage('b', 'agent', false)]))).toBe(false)
    expect(valid(group([stage('a', 'agent', false), stage('b', 'agent', false)]))).toBe(true)
    expect(
      valid(group([stage('a', 'agent'), stage('b', 'agent'), stage('c', 'agent', false)])),
    ).toBe(true)
  })

  it('leaves them out of the steps of a new task', () => {
    const steps = pipelineSchema.parse([
      stage('context', 'context'),
      stage('plan', 'agent', false),
      { id: 'build', parallel: [stage('a', 'agent', false), stage('b', 'agent', false)] },
      {
        id: 'checks',
        parallel: [stage('c', 'agent'), stage('d', 'agent'), stage('e', 'agent', false)],
      },
    ])
    expect(stepRefsOf(activeSteps(steps))).toEqual([
      'context',
      { id: 'checks', parallel: ['c', 'd'] },
    ])
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
  const messages = (value: unknown[]) =>
    pipelineSchema.safeParse(value).error?.issues.map((issue) => issue.message)
  const off = (stage: Stage): Stage => ({ ...stage, enabled: false })

  it('leaves out stages and paths that are off, and runs a lone Otherwise path as steps', () => {
    const docs: Stage = { id: 'write-docs', kind: 'agent', gate: 'auto' }
    const when = small.when
    const three: StageBranch = {
      id: 'after-plan',
      paths: [
        { id: 'a', when, stages: [implement, off(testing)] },
        { id: 'b', when, stages: [off(review)] },
        { id: 'c', stages: [docs] },
      ],
    }
    expect(activeSteps([plan, three, release])).toEqual([
      plan,
      {
        id: 'after-plan',
        paths: [
          { id: 'a', when, stages: [implement] },
          { id: 'c', stages: [docs] },
        ],
      },
      release,
    ])
    const lone: StageBranch = {
      id: 'after-plan',
      paths: [
        { id: 'a', when, stages: [off(implement)] },
        { id: 'b', stages: [review, testing] },
      ],
    }
    expect(activeSteps([plan, lone, release])).toEqual([plan, review, testing, release])
    expect(messages([plan, lone, release])).toBeUndefined()
    const none: StageBranch = {
      id: 'after-plan',
      paths: [
        { id: 'a', when, stages: [off(review)] },
        { id: 'b', stages: [off(testing)] },
      ],
    }
    expect(activeSteps([plan, none, release])).toEqual([plan, release])
    expect(messages([plan, none, release])).toBeUndefined()
  })
})
