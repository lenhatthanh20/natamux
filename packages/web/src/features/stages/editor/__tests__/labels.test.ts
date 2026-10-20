import { DEFAULT_PIPELINE, type Stage, type Step } from '@natamux/core'
import { describe, expect, it } from 'vitest'
import { setOtherwise } from '../pipeline-editor/branches'
import { findBranch } from '../pipeline-editor/find'
import { branchLabel, modelOf, pipelineLabel, stageRunner } from '../pipeline-editor/labels'
import { setEnabled } from '../pipeline-editor/stage-fields'
import { BRANCHED } from './pipeline-editor-fixtures'

describe('pipelineLabel', () => {
  it('names the stages that are on, in order, with groups side by side', () => {
    expect(pipelineLabel(DEFAULT_PIPELINE)).toBe(
      'Pipeline: Context, then Spec, then Plan, then Test spec and Implement side by side, then Automated tests, then Review, then Release',
    )
    expect(pipelineLabel(setEnabled(DEFAULT_PIPELINE, 'plan', false))).not.toContain('Plan')
  })
})

describe('branches: shell fields and labels', () => {
  it('says each branch, with its paths, for a screen reader', () => {
    expect(pipelineLabel(BRANCHED)).toBe(
      'Pipeline: Context, then Plan, then After plan, one path of 2: Small change if plan.md "Size" contains "small"; Review PR if test -f .no-review does not succeed, then Release',
    )
    const otherwise = setOtherwise(BRANCHED, 'after-plan', 'pr-review', true)
    const branch = findBranch(otherwise, 'after-plan')
    expect(branch && branchLabel(branch)).toBe(
      'After plan, one path of 2: Small change if plan.md "Size" contains "small"; Review PR, otherwise',
    )
  })
})

describe('modelOf', () => {
  it('reads the model from the words of a command line', () => {
    expect(modelOf('--model sonnet')).toBe('sonnet')
    expect(modelOf('--model=opus')).toBe('opus')
    expect(modelOf('-m gpt-5')).toBe('gpt-5')
    expect(modelOf('--verbose  --model gpt-5-codex --yolo')).toBe('gpt-5-codex')
  })

  it('finds no model when the args name none', () => {
    expect(modelOf(undefined)).toBeUndefined()
    expect(modelOf('')).toBeUndefined()
    expect(modelOf('--verbose')).toBeUndefined()
    expect(modelOf('--verbose --model')).toBeUndefined()
    expect(modelOf('--model --verbose')).toBeUndefined()
    expect(modelOf('--model=')).toBeUndefined()
  })
})

describe('stageRunner', () => {
  const own = (stage: Partial<Stage>): Step[] => [
    { id: 'plan', kind: 'agent', gate: 'manual' },
    { id: 'my-check', kind: 'agent', gate: 'auto', ...stage },
  ]
  const runner = (steps: Step[], id = 'my-check') => {
    const stage = steps.find((step): step is Stage => step.id === id) as Stage
    return stageRunner(steps, stage)
  }

  it('gives the agent and model of a stage that starts its own agent', () => {
    expect(runner(own({ agent: 'codex', args: '--model gpt-5-codex' }))).toEqual({
      agent: 'codex',
      model: 'gpt-5-codex',
    })
    expect(runner(own({ agent: 'cursor' }))).toEqual({ agent: 'cursor' })
    expect(runner(own({ args: '--model=opus' }))).toEqual({ agent: null, model: 'opus' })
    expect(runner(own({ args: '--verbose' }))).toEqual({ agent: null })
  })

  it('gives nothing for a stage that keeps the task agent, or whose agent is not used', () => {
    expect(runner(own({}))).toBeUndefined()
    const shellOne: Step[] = [{ id: 'implement', kind: 'agent', gate: 'auto', agent: 'codex' }]
    expect(runner(shellOne, 'implement')).toBeUndefined()
  })
})
