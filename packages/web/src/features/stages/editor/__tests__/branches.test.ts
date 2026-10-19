import { DEFAULT_PIPELINE, pipelineIssues } from '@natamux/core'
import { describe, expect, it } from 'vitest'
import {
  addBranch,
  addPath,
  blankCondition,
  conditionProblems,
  editCondition,
  removePath,
  retypeCondition,
  setBranchName,
  setOtherwise,
  setPathCondition,
  setPathName,
} from '../pipeline-editor/branches'
import { findBranch, findStage } from '../pipeline-editor/find'
import { BRANCHED, ids } from './pipeline-editor-fixtures'

describe('addBranch', () => {
  it('adds a branch after the step of the id, with a path with a condition and an Otherwise path', () => {
    const steps = addBranch(DEFAULT_PIPELINE, 'plan')
    expect(ids(steps).slice(2, 5)).toEqual([
      'plan',
      {
        id: 'branch',
        paths: [
          { id: 'path-1', stages: [] },
          { id: 'path-2', stages: [] },
        ],
      },
      { id: 'build', parallel: ['test-spec', 'implement'] },
    ])
    const branch = findBranch(steps, 'branch')
    expect(branch?.paths[0]?.when).toEqual({ type: 'output', file: 'plan.md', contains: '' })
    expect(branch?.paths[1]?.when).toBeUndefined()
  })
  it('goes after the branch when the stage is in a path, and before the release at the end', () => {
    expect(ids(addBranch(BRANCHED, 'testing'))[3]).toMatchObject({ id: 'branch' })
    expect(ids(addBranch(BRANCHED, 'release')).slice(-2)).toEqual([
      expect.objectContaining({ id: 'branch' }),
      'release',
    ])
    expect(ids(addBranch(BRANCHED)).slice(-2)).toEqual([
      expect.objectContaining({ id: 'branch' }),
      'release',
    ])
  })
  it('gives ids no stage, group or path uses', () => {
    const once = addBranch(BRANCHED, 'plan')
    const twice = addBranch(once, 'plan')
    const all = twice.flatMap((step) =>
      'paths' in step ? [step.id, ...step.paths.map((path) => path.id)] : [],
    )
    expect(new Set(all).size).toBe(all.length)
    const branch = findBranch(twice, 'branch-2')
    expect(branch?.paths.map((path) => path.id)).toEqual(['path-3', 'path-4'])
  })
  it('is an error until each path has a stage', () => {
    const issues = pipelineIssues(addBranch(BRANCHED.slice(0, 2), 'plan'))
    expect(issues.some((issue) => issue.stage === 'branch' && issue.level === 'error')).toBe(true)
  })
})

describe('addPath and removePath', () => {
  it('adds a path with a condition at the end, before an Otherwise path', () => {
    const three = addPath(BRANCHED, 'after-plan')
    expect(findBranch(three, 'after-plan')?.paths.map((path) => path.id)).toEqual([
      'small',
      'pr-review',
      'path-3',
    ])
    expect(findBranch(three, 'after-plan')?.paths[2]?.when?.type).toBe('output')
    const otherwise = setOtherwise(BRANCHED, 'after-plan', 'pr-review', true)
    const added = addPath(otherwise, 'after-plan')
    expect(findBranch(added, 'after-plan')?.paths.map((path) => path.id)).toEqual([
      'small',
      'path-3',
      'pr-review',
    ])
  })
  it('stops at 4 paths, and for a branch that is not there', () => {
    const four = addPath(addPath(BRANCHED, 'after-plan'), 'after-plan')
    expect(findBranch(four, 'after-plan')?.paths).toHaveLength(4)
    expect(addPath(four, 'after-plan')).toBe(four)
    expect(addPath(BRANCHED, 'plan')).toBe(BRANCHED)
  })
  it('removes a path with its stages', () => {
    const steps = removePath(BRANCHED, 'after-plan', 'small')
    expect(ids(steps)[2]).toEqual({
      id: 'after-plan',
      paths: [{ id: 'pr-review', stages: ['review'] }],
    })
    expect(findStage(steps, 'implement')).toBeUndefined()
  })
  it('keeps the last path, and ignores an unknown branch', () => {
    const one = removePath(BRANCHED, 'after-plan', 'small')
    expect(removePath(one, 'after-plan', 'pr-review')).toBe(one)
    expect(removePath(BRANCHED, 'nope', 'small')).toBe(BRANCHED)
  })
})

describe('conditions', () => {
  const when = { type: 'output', file: 'plan.md', contains: 'small' } as const
  it('sets the condition, and does not hold with not true', () => {
    const command = { type: 'command', run: 'true' } as const
    const steps = setPathCondition(BRANCHED, 'after-plan', 'small', command, true)
    expect(findBranch(steps, 'after-plan')?.paths[0]).toMatchObject({
      when: { type: 'command', run: 'true' },
      not: true,
    })
    const holds = setPathCondition(steps, 'after-plan', 'small', when)
    expect(findBranch(holds, 'after-plan')?.paths[0]).not.toHaveProperty('not')
    expect(findBranch(holds, 'after-plan')?.paths[0]?.when).toEqual(when)
  })
  it('leaves the other paths and an unknown branch alone', () => {
    const steps = setPathCondition(BRANCHED, 'after-plan', 'small', when)
    expect(findBranch(steps, 'after-plan')?.paths[1]).toEqual(
      findBranch(BRANCHED, 'after-plan')?.paths[1],
    )
    expect(setPathCondition(BRANCHED, 'nope', 'small', when)).toBe(BRANCHED)
  })
  it('edits one field, and leaves an empty section out', () => {
    expect(editCondition(when, 'file', 'notes.md')).toEqual({ ...when, file: 'notes.md' })
    expect(editCondition(when, 'contains', 'big')).toEqual({ ...when, contains: 'big' })
    expect(editCondition(when, 'section', 'Size')).toEqual({ ...when, section: 'Size' })
    expect(editCondition({ ...when, section: 'Size' }, 'section', ' ')).toEqual(when)
    const command = { type: 'command', run: 'a' } as const
    expect(editCondition(command, 'run', 'b')).toEqual({ type: 'command', run: 'b' })
    expect(editCondition(command, 'file', 'b')).toEqual(command)
  })
  it('starts a blank condition of a type, and keeps the one of the same type', () => {
    expect(blankCondition('command')).toEqual({ type: 'command', run: '' })
    expect(blankCondition('output')).toEqual({ type: 'output', file: 'plan.md', contains: '' })
    expect(retypeCondition(when, 'output')).toBe(when)
    expect(retypeCondition(when, 'command')).toEqual({ type: 'command', run: '' })
    expect(retypeCondition(undefined, 'command')).toEqual({ type: 'command', run: '' })
  })
  it('names what is wrong in a condition, by field', () => {
    expect(conditionProblems(when)).toEqual({})
    expect(conditionProblems({ type: 'output', file: 'plan', contains: ' ' })).toEqual({
      file: 'A condition file is a markdown file, such as plan.md',
      contains: 'Give the text to look for',
    })
    expect(conditionProblems({ type: 'command', run: '' })).toEqual({
      run: 'Give the command to run',
    })
  })
})

describe('setOtherwise and names', () => {
  it('makes the last path the Otherwise path: no condition, no not', () => {
    const steps = setOtherwise(BRANCHED, 'after-plan', 'pr-review', true)
    const last = findBranch(steps, 'after-plan')?.paths[1]
    expect(last).not.toHaveProperty('when')
    expect(last).not.toHaveProperty('not')
    expect(last?.stages).toHaveLength(1)
  })
  it('gives a blank condition back when it is turned off', () => {
    const otherwise = setOtherwise(BRANCHED, 'after-plan', 'pr-review', true)
    const off = setOtherwise(otherwise, 'after-plan', 'pr-review', false)
    expect(findBranch(off, 'after-plan')?.paths[1]?.when).toEqual(blankCondition('output'))
  })
  it('works on the last path only, and does nothing when it is already so', () => {
    expect(setOtherwise(BRANCHED, 'after-plan', 'small', true)).toBe(BRANCHED)
    expect(setOtherwise(BRANCHED, 'after-plan', 'pr-review', false)).toBe(BRANCHED)
    const otherwise = setOtherwise(BRANCHED, 'after-plan', 'pr-review', true)
    expect(setOtherwise(otherwise, 'after-plan', 'pr-review', true)).toBe(otherwise)
  })
  it('names a branch and a path, and an empty name leaves the field out', () => {
    const branched = setBranchName(BRANCHED, 'after-plan', 'Choose')
    const named = setPathName(branched, 'after-plan', 'small', 'Tiny')
    expect(findBranch(named, 'after-plan')).toMatchObject({ name: 'Choose' })
    expect(findBranch(named, 'after-plan')?.paths[0]?.name).toBe('Tiny')
    const plain = setPathName(setBranchName(named, 'after-plan', ' '), 'after-plan', 'small', '')
    expect(findBranch(plain, 'after-plan')).not.toHaveProperty('name')
    expect(findBranch(plain, 'after-plan')?.paths[0]).not.toHaveProperty('name')
    expect(setPathName(BRANCHED, 'nope', 'small', 'x')).toBe(BRANCHED)
  })
})
