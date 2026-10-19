import { DEFAULT_PIPELINE, pipelineIssues } from '@natamux/core'
import { describe, expect, it } from 'vitest'
import { addBranch } from '../pipeline-editor/branches'
import {
  addCatalogStage,
  addCustomStage,
  catalogChoices,
  customStageProblems,
  idFromName,
  uniqueIssues,
} from '../pipeline-editor/catalog'
import { findStage } from '../pipeline-editor/find'
import { removeStage } from '../pipeline-editor/move'
import { setEnabled, setText } from '../pipeline-editor/stage-fields'
import { BRANCHED, branchRef, ids } from './pipeline-editor-fixtures'

describe('adding and removing', () => {
  it('adds a catalog stage before the release, with its output and sections', () => {
    const steps = addCatalogStage(DEFAULT_PIPELINE, 'security')
    expect(ids(steps).slice(-2)).toEqual(['security', 'release'])
    expect(findStage(steps, 'security')).toEqual({
      id: 'security',
      name: 'Security review',
      kind: 'agent',
      gate: 'manual',
      output: 'security.md',
      sections: ['Findings'],
      ownShell: true,
    })
    const noRelease = removeStage(DEFAULT_PIPELINE, 'release')
    expect(ids(addCatalogStage(noRelease, 'qa')).at(-1)).toBe('qa')
  })
  it('offers only the catalog stages that are not in the pipeline', () => {
    const offered = catalogChoices(DEFAULT_PIPELINE).map((entry) => entry.stage.id)
    expect(offered).toContain('security')
    expect(offered).not.toContain('spec')
    const withoutSpec = removeStage(DEFAULT_PIPELINE, 'spec')
    expect(catalogChoices(withoutSpec).map((entry) => entry.stage.id)).toContain('spec')
  })
  it('adds a stage of your own that waits for approval', () => {
    const steps = addCustomStage(DEFAULT_PIPELINE, {
      name: 'Load test',
      id: 'load-test',
      kind: 'agent',
      output: 'load.md',
    })
    expect(findStage(steps, 'load-test')).toEqual({
      id: 'load-test',
      kind: 'agent',
      gate: 'manual',
      output: 'load.md',
    })
    const named = addCustomStage(DEFAULT_PIPELINE, {
      name: 'Ask legal',
      id: 'legal',
      kind: 'manual',
      output: 'ignored.md',
    })
    expect(findStage(named, 'legal')).toEqual({
      id: 'legal',
      kind: 'manual',
      gate: 'manual',
      name: 'Ask legal',
    })
  })
  it('refuses a custom stage with a used, wrong or catalog id', () => {
    const input = { name: 'Spec', id: 'spec', kind: 'agent' as const, output: '' }
    expect(customStageProblems(DEFAULT_PIPELINE, input).id).toBe('Another stage uses this id')
    expect(customStageProblems(DEFAULT_PIPELINE, { ...input, id: 'Load Test' }).id).toBe(
      'A stage id is lower case words joined by "-"',
    )
    expect(customStageProblems(DEFAULT_PIPELINE, { ...input, id: 'security' }).id).toMatch(
      /^Security review is a stage of the list/,
    )
    expect(customStageProblems(DEFAULT_PIPELINE, { ...input, name: ' ', id: 'x' })).toEqual({
      name: 'Give the stage a name',
    })
    const wrongOutput = { ...input, id: 'x', output: 'x.txt' }
    expect(customStageProblems(DEFAULT_PIPELINE, wrongOutput).output).toBe(
      'An output is a markdown file name, such as spec.md',
    )
    expect(addCustomStage(DEFAULT_PIPELINE, input)).toBe(DEFAULT_PIPELINE)
  })
  it('makes an id from a name', () => {
    expect(idFromName('Load test (staging)')).toBe('load-test-staging')
    expect(idFromName('  Été  ')).toBe('ete')
  })
})

// Cases of .natamux-task/test-spec.md, one block per case.
describe('test spec cases', () => {
  it('C2: adds a stage before Release also when Release is off', () => {
    const steps = addCatalogStage(setEnabled(DEFAULT_PIPELINE, 'release', false), 'research')
    expect(ids(steps).slice(-2)).toEqual(['research', 'release'])
  })
  it('C2: does not offer a stage that is in the file but off', () => {
    const off = setEnabled(DEFAULT_PIPELINE, 'spec', false)
    expect(catalogChoices(off).map((entry) => entry.stage.id)).not.toContain('spec')
  })
  it('C3: refuses the id of a stage that is off, since ids are unique in the file', () => {
    const off = setEnabled(DEFAULT_PIPELINE, 'plan', false)
    const input = { name: 'Plan', id: 'plan', kind: 'agent' as const, output: '' }
    expect(customStageProblems(off, input).id).toBe('Another stage uses this id')
  })
  it('C3: follows the stage id rule of core, where a leading digit is allowed', () => {
    const problem = (id: string) =>
      customStageProblems(DEFAULT_PIPELINE, { name: 'X', id, kind: 'agent', output: '' }).id
    const rule = 'A stage id is lower case words joined by "-"'
    expect(problem('-load')).toBe(rule)
    expect(problem('load_test')).toBe(rule)
    expect(problem('load!')).toBe(rule)
    expect(problem('load test')).toBe(rule)
    expect(problem('a'.repeat(41))).toBe('An id has 40 characters at most')
    expect(problem('1st-check')).toBeUndefined()
    expect(problem('a'.repeat(40))).toBeUndefined()
  })
  it('C3: makes a valid id from non-ASCII names, or none', () => {
    expect(idFromName('Kiểm thử')).toBe('kiem-thu')
    expect(idFromName('日本語')).toBe('')
    const problems = customStageProblems(DEFAULT_PIPELINE, {
      name: '日本語',
      id: idFromName('日本語'),
      kind: 'agent',
      output: '',
    })
    expect(problems.id).toBe('Give the stage an id')
  })
  it('C3: takes only a markdown file name as output, never a path', () => {
    const problem = (output: string) =>
      customStageProblems(DEFAULT_PIPELINE, { name: 'X', id: 'x', kind: 'agent', output }).output
    const rule = 'An output is a markdown file name, such as spec.md'
    expect(problem('../x.md')).toBe(rule)
    expect(problem('a/b.md')).toBe(rule)
    expect(problem('notes.txt')).toBe(rule)
    expect(problem('load-test.notes.md')).toBeUndefined()
  })
  it('C12: treats a command of spaces as empty', () => {
    const steps = setText(addCatalogStage(DEFAULT_PIPELINE, 'lint'), 'lint', 'command', '   ')
    expect(findStage(steps, 'lint')).not.toHaveProperty('command')
    const kept = setText(steps, 'lint', 'command', 'pnpm lint && echo "ok" | cat')
    expect(findStage(kept, 'lint')?.command).toBe('pnpm lint && echo "ok" | cat')
  })
})

describe('branches: drop, add and remove', () => {
  it('adds a catalog stage to the end of a path', () => {
    expect(ids(addCatalogStage(BRANCHED, 'qa', 'pr-review'))[2]).toEqual(
      branchRef(['implement', 'testing'], ['review', 'qa']),
    )
    expect(ids(addCatalogStage(BRANCHED, 'qa'))[3]).toBe('qa')
  })
  it('does not add a used stage, a release or context stage, or to an unknown path', () => {
    expect(addCatalogStage(BRANCHED, 'implement', 'pr-review')).toBe(BRANCHED)
    expect(addCatalogStage(BRANCHED, 'qa', 'nope')).toBe(BRANCHED)
    const bare = BRANCHED.filter((step) => step.id !== 'release' && step.id !== 'context')
    expect(addCatalogStage(bare, 'release', 'small')).toBe(bare)
    expect(addCatalogStage(bare, 'context', 'small')).toBe(bare)
  })
  it('does not offer a stage that is in a path', () => {
    const choices = catalogChoices(BRANCHED).map((entry) => entry.stage.id)
    expect(choices).not.toContain('implement')
    expect(choices).not.toContain('review')
    expect(choices).toContain('qa')
  })
})

describe('uniqueIssues', () => {
  it('shows a message of a branch once, however many paths break the rule', () => {
    const issues = pipelineIssues(addBranch(BRANCHED.slice(0, 2), 'plan'))
    const messages = uniqueIssues(issues).map((issue) => `${issue.stage}: ${issue.message}`)
    expect(new Set(messages).size).toBe(messages.length)
    expect(messages.some((message) => message.startsWith('branch: '))).toBe(true)
  })
  it('keeps the same message on another stage', () => {
    const error = (stage: string) => ({ stage, message: 'Wrong', level: 'error' as const })
    expect(uniqueIssues([error('a'), error('a'), error('b')])).toEqual([error('a'), error('b')])
  })
})
