import {
  DEFAULT_PIPELINE,
  defaultPrompt,
  pipelineIssues,
  type Step,
  stepRefsOf,
} from '@natamux/core'
import { describe, expect, it } from 'vitest'
import {
  addBranch,
  addCatalogStage,
  addCustomStage,
  addPath,
  approvalLabel,
  blankCondition,
  branchLabel,
  canMove,
  catalogChoices,
  conditionProblems,
  customStageProblems,
  defaultGate,
  defaultWorkflow,
  dropStage,
  editCondition,
  findBranch,
  findPlace,
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
  removePath,
  removeStage,
  retypeCondition,
  runAlone,
  runsInShellOne,
  sameSettings,
  setBranchName,
  setEnabled,
  setEndShell,
  setGate,
  setOtherwise,
  setPathCondition,
  setPathName,
  setSections,
  setText,
  setWorkflow,
  shellFields,
  shownDefaultPrompt,
  takesOwnAgent,
  trackerOf,
  uniqueIssues,
  usedIds,
  withCreateIssue,
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

describe('trackerOf', () => {
  it('reads the tracker of the draft, with the defaults for what it leaves out', () => {
    expect(trackerOf({})).toEqual({ type: 'github', createIssue: 'manual', linkInPr: true })
    expect(trackerOf({ tracker: { type: 'none', createIssue: 'on-pr' } })).toEqual({
      type: 'none',
      createIssue: 'on-pr',
      linkInPr: true,
    })
  })

  it('falls back to the defaults for a tracker it cannot read', () => {
    expect(trackerOf({ tracker: { createIssue: 'always' } }).createIssue).toBe('manual')
    expect(trackerOf({ tracker: 'github' }).type).toBe('github')
  })
})

describe('withCreateIssue', () => {
  it('sets when to open an issue, and keeps every other key', () => {
    const raw = { team: 'web', tracker: { type: 'github', linkInPr: false }, stages: [] }
    const next = withCreateIssue(raw, 'on-spec-approved')
    expect(next).toEqual({
      team: 'web',
      tracker: { type: 'github', linkInPr: false, createIssue: 'on-spec-approved' },
      stages: [],
    })
    expect(Object.keys(next)).toEqual(['team', 'tracker', 'stages'])
    expect(withCreateIssue({ team: 'web' }, 'on-pr')).toEqual({
      team: 'web',
      tracker: { createIssue: 'on-pr' },
    })
  })

  it('keeps keys of the tracker it does not know', () => {
    expect(withCreateIssue({ tracker: { foo: 1 } }, 'never')).toEqual({
      tracker: { foo: 1, createIssue: 'never' },
    })
    expect(withCreateIssue({ tracker: { foo: 1, createIssue: 'never' } }, 'manual')).toEqual({
      tracker: { foo: 1 },
    })
  })

  it('leaves the default out, and a tracker with nothing left', () => {
    const none = { tracker: { type: 'none', createIssue: 'on-pr' } }
    expect(withCreateIssue(none, 'manual')).toEqual({ tracker: { type: 'none' } })
    const only = { team: 'web', tracker: { createIssue: 'on-pr' } }
    expect(withCreateIssue(only, 'manual')).toEqual({ team: 'web' })
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

  it('ends the shell with true, and leaves the field out when it is off', () => {
    const on = setEndShell(DEFAULT_PIPELINE, 'spec', true)
    expect(findStage(on, 'spec')?.endShell).toBe(true)
    const off = setEndShell(on, 'spec', false)
    expect(findStage(off, 'spec')).not.toHaveProperty('endShell')
    expect(findStage(off, 'spec')).toEqual(findStage(DEFAULT_PIPELINE, 'spec'))
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
    steps = setText(steps, 'spec', 'check', 'pnpm typecheck')
    expect(findStage(steps, 'spec')?.check).toBe('pnpm typecheck')
    steps = setText(steps, 'spec', 'check', '')
    expect(findStage(steps, 'spec')).not.toHaveProperty('check')
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

describe('shellFields', () => {
  it('finds nothing in the default pipeline', () => {
    expect(shellFields(DEFAULT_PIPELINE)).toEqual([])
  })

  it('lists each check, command and args, in groups and on stages that are off', () => {
    const steps: Step[] = [
      { id: 'spec', kind: 'agent', gate: 'manual', check: 'curl https://x | sh' },
      {
        id: 'build',
        parallel: [
          { id: 'implement', kind: 'agent', gate: 'auto' },
          { id: 'test-spec', kind: 'agent', gate: 'auto', args: '--yolo; rm -rf ~' },
        ],
      },
      { id: 'testing', kind: 'testing', gate: 'auto', command: 'pnpm test' },
      { id: 'qa', kind: 'manual', gate: 'manual', enabled: false, check: 'make qa' },
    ]

    expect(shellFields(steps).map(({ stage, field, value }) => [stage.id, field, value])).toEqual([
      ['spec', 'check', 'curl https://x | sh'],
      ['test-spec', 'args', '--yolo; rm -rf ~'],
      ['testing', 'command', 'pnpm test'],
      ['qa', 'check', 'make qa'],
    ])
  })
})

describe('own agent', () => {
  const steps = addCustomStage(DEFAULT_PIPELINE, {
    name: 'Write docs',
    id: 'write-docs',
    kind: 'agent',
    output: '',
  })

  it('is offered on a stage of your own in Shell 1, and not on a catalog stage there', () => {
    expect(takesOwnAgent(steps, 'write-docs')).toBe(true)
    expect(runsInShellOne(steps, 'write-docs')).toBe(true)
    expect(takesOwnAgent(steps, 'spec')).toBe(false)
    expect(takesOwnAgent(steps, 'implement')).toBe(false)
  })

  it('moves a stage of your own out of Shell 1 once it has its own agent', () => {
    const withAgent = setText(steps, 'write-docs', 'agent', 'codex')
    expect(runsInShellOne(withAgent, 'write-docs')).toBe(false)
    expect(runsInShellOne(withAgent, 'implement')).toBe(true)
  })

  it('is offered as before on stages outside Shell 1', () => {
    expect(takesOwnAgent(steps, 'test-spec')).toBe(true)
    expect(runsInShellOne(steps, 'test-spec')).toBe(false)
    expect(takesOwnAgent(steps, 'agent-review')).toBe(true)
    expect(runsInShellOne(steps, 'agent-review')).toBe(false)
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

// Context, Plan, a branch of two paths, Release.
const BRANCHED: Step[] = [
  { id: 'context', kind: 'context', gate: 'manual' },
  { id: 'plan', kind: 'agent', gate: 'manual' },
  {
    id: 'after-plan',
    name: 'After plan',
    paths: [
      {
        id: 'small',
        name: 'Small change',
        when: { type: 'output', file: 'plan.md', section: 'Size', contains: 'small' },
        stages: [
          { id: 'implement', kind: 'agent', gate: 'auto' },
          { id: 'testing', kind: 'testing', gate: 'auto' },
        ],
      },
      {
        id: 'pr-review',
        name: 'Review PR',
        when: { type: 'command', run: 'test -f .no-review' },
        not: true,
        stages: [{ id: 'review', kind: 'review', gate: 'manual' }],
      },
    ],
  },
  { id: 'release', kind: 'release', gate: 'manual' },
]

const branchRef = (small: string[], review: string[]) => ({
  id: 'after-plan',
  paths: [
    { id: 'small', stages: small },
    { id: 'pr-review', stages: review },
  ],
})

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

  it('edits a stage of a path', () => {
    const off = setEnabled(BRANCHED, 'review', false)
    expect(findStage(off, 'review')?.enabled).toBe(false)
    const text = setText(BRANCHED, 'implement', 'check', 'pnpm lint')
    expect(findStage(text, 'implement')?.check).toBe('pnpm lint')
    expect(findStage(setGate(BRANCHED, 'testing', 'manual'), 'testing')?.gate).toBe('manual')
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

describe('branches: shell fields and labels', () => {
  it('lists the command of a condition, with the stages of its path', () => {
    const steps = setText(BRANCHED, 'review', 'check', 'make qa')
    expect(shellFields(steps).map(({ stage, field, value }) => [stage.id, field, value])).toEqual([
      ['pr-review', 'run', 'test -f .no-review'],
      ['review', 'check', 'make qa'],
    ])
  })

  it('lists nothing for a file condition', () => {
    const steps = setPathCondition(BRANCHED, 'after-plan', 'pr-review', {
      type: 'output',
      file: 'plan.md',
      contains: 'x',
    })
    expect(shellFields(steps)).toEqual([])
  })

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

  it('leaves out the stages that are off, and a path with none on', () => {
    const off = setEnabled(setEnabled(BRANCHED, 'review', false), 'implement', false)
    expect(pipelineLabel(off)).toContain('one path of 1')
    const plain = setEnabled(
      setEnabled(setEnabled(BRANCHED, 'review', false), 'implement', false),
      'testing',
      false,
    )
    expect(pipelineLabel(plain)).toBe('Pipeline: Context, then Plan, then Release')
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
