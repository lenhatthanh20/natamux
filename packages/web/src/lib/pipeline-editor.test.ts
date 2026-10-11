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
  dropStage,
  findStage,
  idFromName,
  isDefaultGate,
  joinAbove,
  joinProblem,
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
    let steps = dropStage(DEFAULT_PIPELINE, 'plan', 'review', 'before')
    steps = joinAbove(steps, 'plan')
    expect(ids(steps)[2]).toEqual({ id: 'build', parallel: ['test-spec', 'implement', 'plan'] })
    steps = addCatalogStage(steps, 'docs')
    steps = dropStage(steps, 'docs', 'review', 'before')
    steps = joinAbove(steps, 'docs')
    steps = addCatalogStage(steps, 'refactor')
    steps = dropStage(steps, 'refactor', 'review', 'before')
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
      'Pipeline: Context, then Spec, then Plan, then Test spec and Implement side by side, then Review, then Testing, then Release',
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
