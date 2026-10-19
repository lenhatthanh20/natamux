import { DEFAULT_PIPELINE } from '@natamux/core'
import { describe, expect, it } from 'vitest'
import { setOtherwise } from '../pipeline-editor/branches'
import { findBranch } from '../pipeline-editor/find'
import { branchLabel, pipelineLabel } from '../pipeline-editor/labels'
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
