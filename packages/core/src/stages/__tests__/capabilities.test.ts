import { describe, expect, it } from 'vitest'
import {
  canRunWorkflow,
  codeStageBefore,
  offersCi,
  offersWorkflow,
  runsWorkflow,
  stageName,
} from '../capabilities.js'
import { DEFAULT_PIPELINE } from '../default-pipeline.js'
import { pipelineSchema } from '../pipeline-schema.js'
import { type BranchPath, type Stage, type StageBranch, type Step, stagesOf } from '../schema.js'

describe('stageName', () => {
  it('uses the name of the catalog for a catalog stage', () => {
    expect(stageName({ id: 'clarify' })).toBe('Open questions')
    expect(stageName({ id: 'clarify', name: 'Questions' })).toBe('Questions')
  })

  it('names the testing and review stages by who does the work', () => {
    expect(stageName({ id: 'testing' })).toBe('Automated tests')
    expect(stageName({ id: 'qa' })).toBe('Manual testing')
    expect(stageName({ id: 'agent-review' })).toBe('Review')
    expect(stageName({ id: 'review' })).toBe('Manual review')
  })

  it('uses the name, else the id in words', () => {
    expect(stageName({ id: 'test-spec' })).toBe('Test spec')
    expect(stageName({ id: 'build', name: 'Test spec and Implement' })).toBe(
      'Test spec and Implement',
    )
  })
})

describe('a check on CI', () => {
  it('is offered on Implement and Automated tests of the default pipeline', () => {
    const offered = DEFAULT_PIPELINE.flatMap(stagesOf).filter(offersCi)
    expect(offered.map((stage) => stage.id)).toEqual(['implement', 'testing'])
  })

  it('is allowed only on a stage that changes code or runs tests', () => {
    const issues = (stage: object) =>
      pipelineSchema.safeParse([{ id: 'x', gate: 'auto', ci: true, ...stage }]).error?.issues
    expect(issues({ kind: 'agent' })).toBeUndefined()
    expect(issues({ kind: 'testing' })).toBeUndefined()
    for (const stage of [
      { kind: 'agent', output: 'notes.md' },
      { kind: 'review' },
      { kind: 'manual' },
    ]) {
      expect(issues(stage)?.[0]?.message).toBe(
        'Only a stage that changes code or runs tests can be checked on CI, and "x" cannot',
      )
    }
    expect(issues({ kind: 'review', ci: false })).toBeUndefined()
  })

  it('finds the last stage that changes code before a stage', () => {
    const target = (steps: Step[], id: string) => codeStageBefore(steps, id)?.id
    expect(target(DEFAULT_PIPELINE, 'testing')).toBe('implement')
    expect(target(DEFAULT_PIPELINE, 'implement')).toBeUndefined()
    const steps: Step[] = [
      { id: 'spec', kind: 'agent', gate: 'manual', output: 'spec.md' },
      {
        id: 'build',
        parallel: [
          { id: 'code', kind: 'agent', gate: 'auto' },
          { id: 'notes', kind: 'agent', gate: 'auto', output: 'notes.md' },
        ],
      },
      { id: 'testing', kind: 'testing', gate: 'auto', ci: true },
    ]
    expect(target(steps, 'testing')).toBe('code')
    expect(target(steps, 'spec')).toBeUndefined()
    expect(target(steps, 'missing')).toBeUndefined()
    // The last one wins; stages that write a file or wait for a person are skipped.
    const later: Step[] = [
      { id: 'implement', kind: 'agent', gate: 'auto' },
      { id: 'refactor', kind: 'agent', gate: 'auto' },
      { id: 'notes', kind: 'agent', gate: 'auto', output: 'notes.md' },
      { id: 'review', kind: 'review', gate: 'manual' },
      { id: 'testing', kind: 'testing', gate: 'auto', ci: true },
    ]
    expect(target(later, 'testing')).toBe('refactor')
  })
})

describe('workflows', () => {
  const stage = (id: string) => DEFAULT_PIPELINE.flatMap(stagesOf).find((each) => each.id === id)

  it('offers a workflow on plan and implement of the default pipeline only', () => {
    const offered = DEFAULT_PIPELINE.flatMap(stagesOf)
      .filter(offersWorkflow)
      .map((each) => each.id)
    expect(offered).toEqual(['plan', 'implement'])
  })

  it('offers a workflow on a stage of your own that asks for it, and not without the field', () => {
    expect(offersWorkflow({ kind: 'agent', workflow: true })).toBe(true)
    expect(offersWorkflow({ kind: 'agent' })).toBe(false)
    expect(offersWorkflow({ kind: 'agent', workflow: false })).toBe(false)
    expect(offersWorkflow({ kind: 'review', workflow: true })).toBe(false)
  })

  it('lets only Claude Code run a workflow; a task with no agent gets Claude Code', () => {
    expect(canRunWorkflow('claude-code')).toBe(true)
    expect(canRunWorkflow(null)).toBe(true)
    for (const agent of ['codex', 'gemini', 'copilot', 'cursor']) {
      expect(canRunWorkflow(agent)).toBe(false)
    }
  })

  it('runs a workflow when the stage offers it, the task chose it and the agent can', () => {
    const implement = stage('implement')
    if (implement === undefined) {
      throw new Error('The default pipeline has no implement stage')
    }
    expect(runsWorkflow(implement, ['implement'], 'claude-code')).toBe(true)
    expect(runsWorkflow(implement, [], 'claude-code')).toBe(false)
    expect(runsWorkflow(implement, ['plan'], 'claude-code')).toBe(false)
    expect(runsWorkflow(implement, ['implement'], 'codex')).toBe(false)
    expect(runsWorkflow({ ...implement, workflow: false }, ['implement'], 'claude-code')).toBe(
      false,
    )
  })
})

describe('a branch', () => {
  const plan: Stage = { id: 'plan', kind: 'agent', gate: 'manual', output: 'plan.md' }
  const implement: Stage = { id: 'implement', kind: 'agent', gate: 'auto' }
  const testing: Stage = { id: 'testing', kind: 'testing', gate: 'auto' }
  const review: Stage = { id: 'review', kind: 'review', gate: 'manual' }
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

  it('finds the stage that changes code only on the way the task went', () => {
    const loadTest: Stage = { id: 'load-test', kind: 'testing', gate: 'auto', ci: true }
    const all: Step[] = [plan, branch, loadTest]
    expect(codeStageBefore(all, 'testing')?.id).toBe('implement')
    expect(codeStageBefore(all, 'implement')).toBeUndefined()
    expect(codeStageBefore(all, 'load-test')?.id).toBe('implement')
    expect(codeStageBefore(all, 'load-test', { 'after-plan': 'small' })?.id).toBe('implement')
    expect(codeStageBefore(all, 'load-test', { 'after-plan': 'pr-review' })).toBeUndefined()
    expect(codeStageBefore(all, 'load-test', { 'after-plan': '' })).toBeUndefined()
  })
})
