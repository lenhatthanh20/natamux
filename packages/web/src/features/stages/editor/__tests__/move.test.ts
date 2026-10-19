import { DEFAULT_PIPELINE, pipelineIssues, type Step } from '@natamux/core'
import { describe, expect, it } from 'vitest'
import { addCatalogStage } from '../pipeline-editor/catalog'
import { findStage, groupOf, usedIds } from '../pipeline-editor/find'
import {
  canMove,
  dropStage,
  joinAbove,
  joinProblem,
  moveStage,
  removeStage,
  runAlone,
} from '../pipeline-editor/move'
import { setEnabled } from '../pipeline-editor/stage-fields'
import { BRANCHED, branchRef, ids } from './pipeline-editor-fixtures'

describe('moveStage', () => {
  it('swaps a stage with the step above or below', () => {
    expect(ids(moveStage(DEFAULT_PIPELINE, 'plan', -1)).slice(0, 3)).toEqual([
      'context',
      'plan',
      'spec',
    ])
    expect(ids(moveStage(DEFAULT_PIPELINE, 'plan', 1)).slice(2, 4)).toEqual([
      { id: 'build', parallel: ['test-spec', 'implement'] },
      'plan',
    ])
  })
  it('moves a stage within its group, and out of it at the edge', () => {
    expect(ids(moveStage(DEFAULT_PIPELINE, 'implement', -1))[3]).toEqual({
      id: 'build',
      parallel: ['implement', 'test-spec'],
    })
    // A group of one left runs on its own.
    expect(ids(moveStage(DEFAULT_PIPELINE, 'test-spec', -1)).slice(2, 5)).toEqual([
      'plan',
      'test-spec',
      'implement',
    ])
    expect(ids(moveStage(DEFAULT_PIPELINE, 'implement', 1)).slice(3, 5)).toEqual([
      'test-spec',
      'implement',
    ])
  })
  it('does nothing at the ends', () => {
    expect(moveStage(DEFAULT_PIPELINE, 'context', -1)).toBe(DEFAULT_PIPELINE)
    expect(canMove(DEFAULT_PIPELINE, 'context', -1)).toBe(false)
    expect(canMove(DEFAULT_PIPELINE, 'release', 1)).toBe(false)
    expect(canMove(DEFAULT_PIPELINE, 'release', -1)).toBe(true)
  })
})

describe('dropStage', () => {
  it('puts a stage before or after another', () => {
    expect(ids(dropStage(DEFAULT_PIPELINE, 'testing', 'spec', 'before')).slice(0, 3)).toEqual([
      'context',
      'testing',
      'spec',
    ])
    expect(ids(dropStage(DEFAULT_PIPELINE, 'context', 'plan', 'after')).slice(0, 3)).toEqual([
      'spec',
      'plan',
      'context',
    ])
  })
  it('reorders the stages of a group without breaking it', () => {
    expect(ids(dropStage(DEFAULT_PIPELINE, 'implement', 'test-spec', 'before'))[3]).toEqual({
      id: 'build',
      parallel: ['implement', 'test-spec'],
    })
  })
  it('lets an agent stage join a group, but puts a review next to it', () => {
    expect(ids(dropStage(DEFAULT_PIPELINE, 'plan', 'implement', 'after'))[2]).toEqual({
      id: 'build',
      parallel: ['test-spec', 'implement', 'plan'],
    })
    expect(ids(dropStage(DEFAULT_PIPELINE, 'testing', 'test-spec', 'before')).slice(2, 5)).toEqual([
      'plan',
      'testing',
      { id: 'build', parallel: ['test-spec', 'implement'] },
    ])
  })
  it('moves a whole group', () => {
    expect(ids(dropStage(DEFAULT_PIPELINE, 'build', 'spec', 'before')).slice(0, 3)).toEqual([
      'context',
      { id: 'build', parallel: ['test-spec', 'implement'] },
      'spec',
    ])
  })
})

describe('run side by side', () => {
  it('makes a new group of a stage and the stage above, with an unused id', () => {
    const steps = joinAbove(DEFAULT_PIPELINE, 'plan')
    expect(ids(steps)[1]).toEqual({ id: 'spec-group', parallel: ['spec', 'plan'] })
    const added = addCatalogStage(addCatalogStage(steps, 'research'), 'clarify')
    const again = joinAbove(added, 'clarify')
    expect(ids(again)).toContainEqual({ id: 'research-group', parallel: ['research', 'clarify'] })
  })
  it('joins the group above, up to 4 stages', () => {
    let steps = dropStage(DEFAULT_PIPELINE, 'plan', 'testing', 'before')
    steps = joinAbove(steps, 'plan')
    expect(ids(steps)[2]).toEqual({ id: 'build', parallel: ['test-spec', 'implement', 'plan'] })
    steps = addCatalogStage(steps, 'docs')
    steps = dropStage(steps, 'docs', 'testing', 'before')
    steps = joinAbove(steps, 'docs')
    steps = addCatalogStage(steps, 'refactor')
    steps = dropStage(steps, 'refactor', 'testing', 'before')
    expect(joinProblem(steps, 'refactor')).toBe('A group holds 4 stages at most')
  })
  it('is not offered for review, testing, context or release stages', () => {
    expect(joinProblem(DEFAULT_PIPELINE, 'testing')).toBe(
      'Only agent and manual stages run side by side',
    )
    expect(joinProblem(DEFAULT_PIPELINE, 'release')).toBe(
      'Only agent and manual stages run side by side',
    )
    expect(joinProblem(DEFAULT_PIPELINE, 'spec')).toBe(
      'Context cannot run side by side with other stages',
    )
    expect(joinProblem(DEFAULT_PIPELINE, 'context')).toBe(
      'Only agent and manual stages run side by side',
    )
    expect(joinProblem(DEFAULT_PIPELINE, 'implement')).toBe(
      'It runs alongside other stages already',
    )
    expect(joinAbove(DEFAULT_PIPELINE, 'testing')).toBe(DEFAULT_PIPELINE)
  })
  it('takes a stage out of its group; a group of one left runs on its own', () => {
    expect(ids(runAlone(DEFAULT_PIPELINE, 'test-spec')).slice(3, 5)).toEqual([
      'implement',
      'test-spec',
    ])
  })
})

// Cases of .natamux-task/test-spec.md, one block per case.
describe('test spec cases', () => {
  it('C4: keeps a stage off when it moves', () => {
    const moved = moveStage(setEnabled(DEFAULT_PIPELINE, 'plan', false), 'plan', -1)
    expect(ids(moved).slice(0, 3)).toEqual(['context', 'plan', 'spec'])
    expect(findStage(moved, 'plan')?.enabled).toBe(false)
  })
  it('C6: joins the stage above even when it is off', () => {
    const steps = joinAbove(setEnabled(DEFAULT_PIPELINE, 'spec', false), 'plan')
    expect(groupOf(steps, 'plan')?.parallel.map((stage) => stage.id)).toEqual(['spec', 'plan'])
  })
  it('C6: lets a manual stage run alongside an agent stage', () => {
    const steps: Step[] = [
      { id: 'docs', kind: 'agent', gate: 'auto' },
      { id: 'qa', kind: 'manual', gate: 'manual' },
    ]
    expect(joinProblem(steps, 'qa')).toBeNull()
    expect(ids(joinAbove(steps, 'qa'))).toEqual([{ id: 'docs-group', parallel: ['docs', 'qa'] }])
  })
  it('C6: dissolves a group of two when one of its stages is removed', () => {
    expect(ids(removeStage(DEFAULT_PIPELINE, 'implement')).slice(2, 5)).toEqual([
      'plan',
      'test-spec',
      'testing',
    ])
  })
  it('C6: offers nothing to the first stage', () => {
    const steps: Step[] = [{ id: 'docs', kind: 'agent', gate: 'auto' }]
    expect(joinProblem(steps, 'docs')).toBe('There is no stage above')
  })
})

describe('branches: moving stages', () => {
  it('moves a stage within its path', () => {
    const swapped = branchRef(['testing', 'implement'], ['review'])
    expect(ids(moveStage(BRANCHED, 'implement', 1))[2]).toEqual(swapped)
    expect(ids(moveStage(BRANCHED, 'testing', -1))[2]).toEqual(swapped)
  })
  it('moves a stage to the next path at the end of its path, and back', () => {
    expect(ids(moveStage(BRANCHED, 'testing', 1))[2]).toEqual(
      branchRef(['implement'], ['testing', 'review']),
    )
    expect(ids(moveStage(BRANCHED, 'review', -1))[2]).toEqual(
      branchRef(['implement', 'testing', 'review'], []),
    )
  })
  it('moves a stage out of the branch at the edge of the first or last path', () => {
    expect(ids(moveStage(BRANCHED, 'review', 1)).slice(2, 5)).toEqual([
      branchRef(['implement', 'testing'], []),
      'review',
      'release',
    ])
    expect(ids(moveStage(BRANCHED, 'implement', -1)).slice(1, 4)).toEqual([
      'plan',
      'implement',
      branchRef(['testing'], ['review']),
    ])
  })
  it('moves a stage next to a branch into its nearest path', () => {
    const steps = moveStage(BRANCHED, 'plan', 1)
    expect(ids(steps)).toEqual([
      'context',
      branchRef(['plan', 'implement', 'testing'], ['review']),
      'release',
    ])
    const after: Step[] = [
      ...BRANCHED.slice(0, 3),
      { id: 'docs', kind: 'agent', gate: 'auto' },
      ...BRANCHED.slice(3),
    ]
    expect(ids(moveStage(after, 'docs', -1))[2]).toEqual(
      branchRef(['implement', 'testing'], ['review', 'docs']),
    )
  })
  it('moves the release over a branch, and the branch like any step', () => {
    expect(ids(moveStage(BRANCHED, 'release', -1)).slice(2)).toEqual([
      'release',
      branchRef(['implement', 'testing'], ['review']),
    ])
    expect(ids(moveStage(BRANCHED, 'after-plan', -1)).slice(1, 3)).toEqual([
      branchRef(['implement', 'testing'], ['review']),
      'plan',
    ])
  })
  it('tells when a stage of a path can move', () => {
    expect(canMove(BRANCHED, 'implement', 1)).toBe(true)
    expect(canMove(BRANCHED, 'review', 1)).toBe(true)
    expect(canMove(BRANCHED, 'release', 1)).toBe(false)
  })
  it('keeps every id once after any move', () => {
    for (const id of ['implement', 'testing', 'review', 'plan']) {
      for (const direction of [-1, 1] as const) {
        const used = [...usedIds(moveStage(BRANCHED, id, direction))]
        expect(used.sort()).toEqual([...usedIds(BRANCHED)].sort())
      }
    }
  })
})

describe('branches: drop, add and remove', () => {
  it('drops a stage before or after a stage of a path', () => {
    expect(ids(dropStage(BRANCHED, 'plan', 'review', 'before'))[1]).toEqual(
      branchRef(['implement', 'testing'], ['plan', 'review']),
    )
    expect(ids(dropStage(BRANCHED, 'testing', 'implement', 'before'))[2]).toEqual(
      branchRef(['testing', 'implement'], ['review']),
    )
    expect(ids(dropStage(BRANCHED, 'implement', 'review', 'after'))[2]).toEqual(
      branchRef(['testing'], ['review', 'implement']),
    )
  })
  it('drops a stage on a path, to its end', () => {
    expect(ids(dropStage(BRANCHED, 'plan', 'pr-review', 'after'))[1]).toEqual(
      branchRef(['implement', 'testing'], ['review', 'plan']),
    )
    const empty = removeStage(BRANCHED, 'review')
    expect(ids(dropStage(empty, 'testing', 'pr-review', 'after'))[2]).toEqual(
      branchRef(['implement'], ['testing']),
    )
  })
  it('drops a stage of a path out of the branch, next to a step', () => {
    expect(ids(dropStage(BRANCHED, 'implement', 'release', 'before')).slice(2, 5)).toEqual([
      branchRef(['testing'], ['review']),
      'implement',
      'release',
    ])
    expect(ids(dropStage(BRANCHED, 'review', 'after-plan', 'after')).slice(2, 4)).toEqual([
      branchRef(['implement', 'testing'], []),
      'review',
    ])
  })
  it('does not drop a group, a context stage or a release stage into a path', () => {
    const grouped: Step[] = [
      ...BRANCHED.slice(0, 3),
      {
        id: 'build',
        parallel: [
          { id: 'docs', kind: 'agent', gate: 'auto' },
          { id: 'i18n', kind: 'agent', gate: 'auto' },
        ],
      },
      ...BRANCHED.slice(3),
    ]
    expect(dropStage(grouped, 'build', 'review', 'before')).toBe(grouped)
    expect(dropStage(BRANCHED, 'context', 'review', 'after')).toBe(BRANCHED)
    expect(dropStage(BRANCHED, 'release', 'pr-review', 'after')).toBe(BRANCHED)
    expect(dropStage(BRANCHED, 'after-plan', 'review', 'after')).toBe(BRANCHED)
  })
  it('removes a stage of a path, and leaves the path with none', () => {
    const steps = removeStage(BRANCHED, 'review')
    expect(ids(steps)[2]).toEqual(branchRef(['implement', 'testing'], []))
    const issues = pipelineIssues(steps)
    expect(issues.some((issue) => issue.stage === 'after-plan' && issue.level === 'error')).toBe(
      true,
    )
    expect(ids(removeStage(BRANCHED, 'after-plan'))).toEqual(['context', 'plan', 'release'])
  })
  it('does not run a stage of a path alongside another', () => {
    expect(joinProblem(BRANCHED, 'testing')).toBe('A path holds single stages only')
    expect(joinAbove(BRANCHED, 'testing')).toBe(BRANCHED)
    const after: Step[] = [
      ...BRANCHED.slice(0, 3),
      { id: 'docs', kind: 'agent', gate: 'auto' },
      ...BRANCHED.slice(3),
    ]
    expect(joinProblem(after, 'docs')).toBe(
      'After plan is a branch; a stage cannot run alongside it',
    )
    expect(joinAbove(after, 'docs')).toBe(after)
  })
})
