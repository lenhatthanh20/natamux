import { DEFAULT_PIPELINE, defaultPrompt, type Step, stepRefsOf } from '@natamux/core'
import { describe, expect, it } from 'vitest'
import {
  addCatalogStage,
  addCustomStage,
  approvalLabel,
  canMove,
  catalogChoices,
  customStageProblems,
  defaultGate,
  defaultWorkflow,
  dropStage,
  findStage,
  groupOf,
  idFromName,
  isDefaultGate,
  joinAbove,
  joinProblem,
  mergeChanges,
  moveStage,
  parseDraft,
  pipelineLabel,
  removeStage,
  runAlone,
  sameSettings,
  setEnabled,
  setGate,
  setSections,
  setText,
  setWorkflow,
  shownDefaultPrompt,
  withSteps,
} from './pipeline-editor'

const ids = (steps: readonly Step[]) => stepRefsOf(steps)

describe('parseDraft', () => {
  it('reads the default pipeline when the file has no stages', () => {
    const draft = parseDraft('{ "tracker": { "type": "none" } }')
    expect(draft.ok && draft.steps).toEqual(DEFAULT_PIPELINE)
  })

  it('refuses text that is not JSON, or not an object', () => {
    const broken = parseDraft('{ "stages": [')
    expect(broken.ok).toBe(false)
    expect(!broken.ok && broken.error).toMatch(/^This is not JSON/)
    expect(parseDraft('[]')).toEqual({
      ok: false,
      error: 'The settings are one JSON object, such as {}.',
    })
  })

  it('refuses stages the list cannot draw', () => {
    const draft = parseDraft('{ "stages": [{ "id": "spec", "kind": "robot", "gate": "auto" }] }')
    expect(!draft.ok && draft.error).toMatch(/^stages, item 1: each stage needs/)
  })

  it('refuses settings outside the stages that break the rules', () => {
    const draft = parseDraft('{ "tracker": { "type": "jira" } }')
    expect(!draft.ok && draft.error).toMatch(/^tracker\.type/)
  })

  it('keeps stages with a wrong field value, for the list to name the problem', () => {
    const draft = parseDraft(
      '{ "stages": [{ "id": "spec", "kind": "agent", "gate": "auto", "output": "x" }] }',
    )
    expect(draft.ok).toBe(true)
  })
})

describe('withSteps', () => {
  it('leaves the stages out when they equal the default, and keeps every other key', () => {
    const raw = { team: 'web', stages: [], tracker: { type: 'none' } }
    expect(withSteps(raw, DEFAULT_PIPELINE)).toEqual({ team: 'web', tracker: { type: 'none' } })
  })

  it('keeps the place of the stages among the keys, or puts them first', () => {
    const steps: Step[] = [{ id: 'implement', kind: 'agent', gate: 'auto' }]
    expect(Object.keys(withSteps({ a: 1, stages: [], b: 2 }, steps))).toEqual(['a', 'stages', 'b'])
    expect(Object.keys(withSteps({ a: 1 }, steps))).toEqual(['stages', 'a'])
  })
})

describe('sameSettings', () => {
  it('compares the settings, not the text', () => {
    expect(sameSettings('{"a":1,"b":[1,2]}', '{\n  "b": [1, 2],\n  "a": 1\n}')).toBe(true)
    expect(sameSettings('{"a":1}', '{"a":2}')).toBe(false)
    expect(sameSettings('{"a":', '{"a":')).toBe(true)
    expect(sameSettings('{"a":', '{"a":1}')).toBe(false)
  })
})

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
    expect(ids(dropStage(DEFAULT_PIPELINE, 'review', 'spec', 'before')).slice(0, 3)).toEqual([
      'context',
      'review',
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
    expect(ids(dropStage(DEFAULT_PIPELINE, 'review', 'test-spec', 'before')).slice(2, 5)).toEqual([
      'plan',
      'review',
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

describe('stage fields', () => {
  it('offers a workflow with true, and leaves the field out when it is off', () => {
    const on = setWorkflow(DEFAULT_PIPELINE, 'spec', true)
    expect(findStage(on, 'spec')?.workflow).toBe(true)
    const off = setWorkflow(DEFAULT_PIPELINE, 'implement', false)
    expect(findStage(off, 'implement')).not.toHaveProperty('workflow')
    expect(findStage(off, 'plan')?.workflow).toBe(true)
  })

  it('takes the workflow of the catalog as the default, and none for a stage of your own', () => {
    expect(defaultWorkflow({ id: 'plan', kind: 'agent' })).toBe(true)
    expect(defaultWorkflow({ id: 'implement', kind: 'agent' })).toBe(true)
    expect(defaultWorkflow({ id: 'spec', kind: 'agent' })).toBe(false)
    expect(defaultWorkflow({ id: 'my-stage', kind: 'agent' })).toBe(false)
    expect(defaultWorkflow({ id: 'plan', kind: 'manual' })).toBe(false)
  })

  it('turns a stage off with enabled false, and on by removing it', () => {
    const off = setEnabled(DEFAULT_PIPELINE, 'plan', false)
    expect(findStage(off, 'plan')?.enabled).toBe(false)
    const on = setEnabled(off, 'plan', true)
    expect(findStage(on, 'plan')).toEqual(findStage(DEFAULT_PIPELINE, 'plan'))
  })

  it('removes a field set to an empty value or to its default', () => {
    let steps = setText(DEFAULT_PIPELINE, 'spec', 'name', 'Product spec')
    expect(findStage(steps, 'spec')?.name).toBe('Product spec')
    steps = setText(steps, 'spec', 'name', 'Spec')
    expect(findStage(steps, 'spec')).not.toHaveProperty('name')
    steps = setText(steps, 'testing', 'command', 'pnpm test')
    expect(findStage(steps, 'testing')?.command).toBe('pnpm test')
    steps = setText(steps, 'testing', 'command', '  ')
    expect(findStage(steps, 'testing')).not.toHaveProperty('command')
    steps = setSections(steps, 'spec', ['Goal', ' ', 'Risks '])
    expect(findStage(steps, 'spec')?.sections).toEqual(['Goal', 'Risks'])
    steps = setSections(steps, 'spec', [])
    expect(findStage(steps, 'spec')).not.toHaveProperty('sections')
  })

  it('keeps a custom prompt, and drops one equal to the default', () => {
    const plan = findStage(DEFAULT_PIPELINE, 'plan')
    let steps = setText(DEFAULT_PIPELINE, 'plan', 'prompt', 'Use {skill:planner} for {title}.')
    expect(findStage(steps, 'plan')?.prompt).toBe('Use {skill:planner} for {title}.')
    steps = setText(steps, 'plan', 'prompt', plan === undefined ? '' : defaultPrompt(plan))
    expect(findStage(steps, 'plan')).not.toHaveProperty('prompt')
  })

  it('knows the default approval of each stage', () => {
    const spec = findStage(DEFAULT_PIPELINE, 'spec')
    expect(spec && isDefaultGate(spec)).toBe(true)
    const auto = findStage(setGate(DEFAULT_PIPELINE, 'spec', 'auto'), 'spec')
    expect(auto && isDefaultGate(auto)).toBe(false)
    expect(defaultGate({ id: 'implement', kind: 'agent' })).toBe('auto')
    expect(defaultGate({ id: 'my-stage', kind: 'agent' })).toBe('manual')
    expect(defaultGate({ id: 'qa', kind: 'manual' })).toBe('manual')
    expect(approvalLabel({ kind: 'release', gate: 'manual' })).toBe(
      'Opens the pull request when you click',
    )
  })
})

describe('pipelineLabel', () => {
  it('names the stages that are on, in order, with groups side by side', () => {
    expect(pipelineLabel(DEFAULT_PIPELINE)).toBe(
      'Pipeline: Context, then Spec, then Plan, then Test spec and Implement side by side, then Automated tests, then Review, then Release',
    )
    expect(pipelineLabel(setEnabled(DEFAULT_PIPELINE, 'plan', false))).not.toContain('Plan')
  })
})

describe('shownDefaultPrompt', () => {
  it('writes the commit rule into the default prompt, as the agent gets it', () => {
    const implement = findStage(DEFAULT_PIPELINE, 'implement')
    const text = implement === undefined ? '' : shownDefaultPrompt(implement)
    expect(text).toContain('Do not commit. Leave the changes for review.')
    expect(text).not.toContain('{commit}')
    const steps = setText(DEFAULT_PIPELINE, 'implement', 'prompt', text)
    expect(findStage(steps, 'implement')).not.toHaveProperty('prompt')
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

  it('C12: treats a command of spaces as empty', () => {
    const steps = setText(addCatalogStage(DEFAULT_PIPELINE, 'lint'), 'lint', 'command', '   ')
    expect(findStage(steps, 'lint')).not.toHaveProperty('command')
    const kept = setText(steps, 'lint', 'command', 'pnpm lint && echo "ok" | cat')
    expect(findStage(kept, 'lint')?.command).toBe('pnpm lint && echo "ok" | cat')
  })

  it('C15: keeps the keys of the new file that the person did not change', () => {
    const start = { tracker: { type: 'github' }, agents: { permissionMode: 'auto' } }
    const draft = { ...start, stages: [{ id: 'implement', kind: 'agent', gate: 'auto' }] }
    const now = { tracker: { type: 'none' }, agents: { permissionMode: 'auto' }, team: 'web' }
    expect(mergeChanges(start, draft, now)).toEqual({
      tracker: { type: 'none' },
      agents: { permissionMode: 'auto' },
      team: 'web',
      stages: draft.stages,
    })
  })

  it('C15: removes a key the person removed, and takes the value of the draft on a clash', () => {
    const start = { stages: [], tracker: { type: 'github' } }
    const draft = { tracker: { type: 'none' } }
    const now = { stages: [{ id: 'spec' }], tracker: { type: 'github', linkInPr: false } }
    expect(mergeChanges(start, draft, now)).toEqual({ tracker: { type: 'none' } })
  })
})
