import { describe, expect, it } from 'vitest'
import { findBranch, findPlace, findStage, groupOf, usedIds } from '../pipeline-editor/find'
import { parseDraft } from '../pipeline-editor/settings-json'
import { BRANCHED } from './pipeline-editor-fixtures'

describe('branches: reading', () => {
  it('reads a branch from the JSON, and refuses one the list cannot draw', () => {
    const draft = parseDraft(JSON.stringify({ stages: BRANCHED }))
    expect(draft.ok && draft.steps).toEqual(BRANCHED)
    const noStages = parseDraft(
      '{ "stages": [{ "id": "after-plan", "paths": [{ "id": "small" }, { "id": "other", "stages": [] }] }] }',
    )
    expect(!noStages.ok && noStages.error).toMatch(
      /^stages, item 1: a branch needs an id and paths/,
    )
    const group = parseDraft(
      '{ "stages": [{ "id": "b", "paths": [{ "id": "p", "stages": [{ "id": "g", "parallel": [] }] }] }] }',
    )
    expect(group.ok).toBe(false)
  })
  it('keeps a path with no stage and a condition with a wrong value, for the issues to name', () => {
    const draft = parseDraft(
      '{ "stages": [{ "id": "b", "paths": [{ "id": "p", "when": { "type": "command", "run": "" }, "stages": [] }, { "id": "q", "stages": [] }] }] }',
    )
    expect(draft.ok).toBe(true)
  })
  it('lists every id: stages, branches and paths', () => {
    expect([...usedIds(BRANCHED)].sort()).toEqual(
      [
        'after-plan',
        'context',
        'implement',
        'plan',
        'pr-review',
        'release',
        'review',
        'small',
        'testing',
      ].sort(),
    )
  })
  it('finds a stage of a path, its path, and its place in it', () => {
    expect(findPlace(BRANCHED, 'testing')).toEqual({ index: 2, lane: null, path: 0, slot: 1 })
    expect(findPlace(BRANCHED, 'review')).toEqual({ index: 2, lane: null, path: 1, slot: 0 })
    expect(findPlace(BRANCHED, 'after-plan')).toEqual({
      index: 2,
      lane: null,
      path: null,
      slot: null,
    })
    expect(findPlace(BRANCHED, 'small')).toBeNull()
    expect(groupOf(BRANCHED, 'testing')).toBeUndefined()
    expect(findStage(BRANCHED, 'review')?.kind).toBe('review')
    expect(findBranch(BRANCHED, 'after-plan')?.paths).toHaveLength(2)
    expect(findBranch(BRANCHED, 'plan')).toBeUndefined()
  })
})
