import { DEFAULT_PIPELINE, defaultPrompt, pipelineIssues, type Step } from '@natamux/core'
import { describe, expect, it } from 'vitest'
import { setPathCondition } from '../pipeline-editor/branches'
import { addCustomStage } from '../pipeline-editor/catalog'
import { findStage } from '../pipeline-editor/find'
import { approvalLabel, pipelineLabel } from '../pipeline-editor/labels'
import {
  defaultGate,
  defaultOwnShell,
  defaultWorkflow,
  isDefaultGate,
  offersOwnShell,
  runsInOwnShell,
  runsInShellOne,
  setEnabled,
  setEndShell,
  setGate,
  setOwnShell,
  setSections,
  setText,
  setWorkflow,
  shellFields,
  shownDefaultPrompt,
  takesOwnAgent,
} from '../pipeline-editor/stage-fields'
import { BRANCHED } from './pipeline-editor-fixtures'

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
  it('is offered on every agent or testing stage in Shell 1', () => {
    expect(takesOwnAgent(steps, 'write-docs')).toBe(true)
    expect(runsInShellOne(steps, 'write-docs')).toBe(true)
    expect(takesOwnAgent(steps, 'spec')).toBe(true)
    expect(takesOwnAgent(steps, 'implement')).toBe(true)
    expect(takesOwnAgent(steps, 'testing')).toBe(true)
    expect(takesOwnAgent(steps, 'context')).toBe(false)
    expect(takesOwnAgent(steps, 'release')).toBe(false)
  })
  it('moves a catalog stage out of Shell 1 with no issue', () => {
    const withAgent = setText(DEFAULT_PIPELINE, 'spec', 'agent', 'codex')
    expect(runsInShellOne(withAgent, 'spec')).toBe(false)
    expect(runsInShellOne(withAgent, 'plan')).toBe(true)
    expect(pipelineIssues(withAgent)).toEqual([])
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

describe('own shell', () => {
  it('is offered on an agent stage with an output outside a group', () => {
    expect(offersOwnShell(DEFAULT_PIPELINE, 'spec')).toBe(true)
    expect(offersOwnShell(DEFAULT_PIPELINE, 'agent-review')).toBe(true)
    // Implement has no output; Testing is not an agent stage; a group ignores it.
    expect(offersOwnShell(DEFAULT_PIPELINE, 'implement')).toBe(false)
    expect(offersOwnShell(DEFAULT_PIPELINE, 'testing')).toBe(false)
    const inGroup = setOwnShell(DEFAULT_PIPELINE, 'test-spec', false)
    expect(offersOwnShell(inGroup, 'test-spec')).toBe(false)
  })
  it('is shown on any stage that has it, so it can be cleared', () => {
    // Test spec in the default group has it from the catalog.
    expect(offersOwnShell(DEFAULT_PIPELINE, 'test-spec')).toBe(true)
    const wrong = setOwnShell(setOwnShell(DEFAULT_PIPELINE, 'testing', true), 'implement', true)
    expect(offersOwnShell(wrong, 'testing')).toBe(true)
    expect(offersOwnShell(wrong, 'implement')).toBe(true)
    const cleared = setOwnShell(wrong, 'testing', false)
    expect(findStage(cleared, 'testing')).not.toHaveProperty('ownShell')
  })
  it('writes true, and off leaves the field out so Spec runs in Terminal 1', () => {
    const on = setOwnShell(DEFAULT_PIPELINE, 'spec', true)
    expect(findStage(on, 'spec')?.ownShell).toBe(true)
    expect(runsInShellOne(on, 'spec')).toBe(false)
    expect(runsInOwnShell(on, 'spec')).toBe(true)
    const off = setOwnShell(on, 'spec', false)
    expect(findStage(off, 'spec')).toEqual(findStage(DEFAULT_PIPELINE, 'spec'))
    expect(runsInShellOne(off, 'spec')).toBe(true)
    expect(runsInOwnShell(off, 'spec')).toBe(false)
  })
  it('keeps the own agent of a catalog stage when it is turned off', () => {
    let steps = setText(DEFAULT_PIPELINE, 'agent-review', 'agent', 'cursor')
    steps = setText(steps, 'agent-review', 'args', '--model x')
    const off = setOwnShell(steps, 'agent-review', false)
    const review = findStage(off, 'agent-review')
    expect(review).not.toHaveProperty('ownShell')
    expect(review).toMatchObject({ agent: 'cursor', args: '--model x' })
    expect(pipelineIssues(off).filter((issue) => issue.stage === 'agent-review')).toEqual([])
  })
  it('keeps the agent of a stage of your own', () => {
    let steps = addCustomStage(DEFAULT_PIPELINE, {
      name: 'Write docs',
      id: 'write-docs',
      kind: 'agent',
      output: 'docs.md',
    })
    steps = setOwnShell(setText(steps, 'write-docs', 'agent', 'codex'), 'write-docs', true)
    const off = setOwnShell(steps, 'write-docs', false)
    expect(findStage(off, 'write-docs')?.agent).toBe('codex')
  })
  it('takes the catalog as the default, and off for a stage of your own', () => {
    expect(defaultOwnShell({ id: 'agent-review' })).toBe(true)
    expect(defaultOwnShell({ id: 'test-spec' })).toBe(true)
    expect(defaultOwnShell({ id: 'spec' })).toBe(false)
    expect(defaultOwnShell({ id: 'write-docs' })).toBe(false)
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

describe('branches: drop, add and remove', () => {
  it('edits a stage of a path', () => {
    const off = setEnabled(BRANCHED, 'review', false)
    expect(findStage(off, 'review')?.enabled).toBe(false)
    const text = setText(BRANCHED, 'implement', 'check', 'pnpm lint')
    expect(findStage(text, 'implement')?.check).toBe('pnpm lint')
    expect(findStage(setGate(BRANCHED, 'testing', 'manual'), 'testing')?.gate).toBe('manual')
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
