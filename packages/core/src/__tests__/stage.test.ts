import { describe, expect, it } from 'vitest'
import { DEFAULT_PROJECT_SETTINGS, projectSettingsSchema } from '../project-settings.js'
import {
  activeSteps,
  type BranchPath,
  canEndShell,
  canRunWorkflow,
  codeStageBefore,
  combinedState,
  DEFAULT_PIPELINE,
  defaultPrompt,
  describeCondition,
  endsShell1,
  enterStep,
  findStep,
  firstPosition,
  isBranch,
  isGroup,
  isNotTaken,
  isStage,
  LEGACY_STEP_REFS,
  offersCi,
  offersWorkflow,
  pathOf,
  pipelineSchema,
  resolveSteps,
  runsWorkflow,
  type Stage,
  type StageBranch,
  type Step,
  stageAgent,
  stageIdSchema,
  stageName,
  stageOfTerminal,
  stagesOf,
  stepIndex,
  stepRefSchema,
  stepRefsOf,
  terminalLane,
  terminalOfStage,
} from '../stage.js'
import type { TaskState } from '../task-state.js'

describe('pipelineSchema', () => {
  it('accepts the default pipeline', () => {
    expect(pipelineSchema.parse(DEFAULT_PIPELINE)).toEqual(DEFAULT_PIPELINE)
  })

  it('rejects a stage id used twice, also inside a group', () => {
    const twice = [
      { id: 'spec', kind: 'agent', gate: 'manual' },
      {
        id: 'build',
        parallel: [
          { id: 'spec', kind: 'agent', gate: 'manual' },
          { id: 'implement', kind: 'agent', gate: 'auto' },
        ],
      },
    ]
    expect(pipelineSchema.safeParse(twice).success).toBe(false)
  })

  it('allows only agent and manual stages, 2 to 4, in a group', () => {
    const group = (parallel: unknown[]) => [{ id: 'build', parallel }]
    const agent = (id: string) => ({ id, kind: 'agent', gate: 'auto' })
    const messages = (value: unknown[]) =>
      pipelineSchema.safeParse(value).error?.issues.map((issue) => issue.message)
    expect(messages(group([agent('a')]))).toContain('A group holds 2 to 4 stages')
    expect(pipelineSchema.safeParse(group([agent('a'), agent('b')])).success).toBe(true)
    expect(messages(group(['a', 'b', 'c', 'd', 'e'].map(agent)))).toEqual([
      'A group holds 2 to 4 stages',
    ])
    expect(
      pipelineSchema.safeParse(group([agent('a'), { id: 'r', kind: 'review', gate: 'manual' }]))
        .success,
    ).toBe(false)
  })

  it('keeps the context first and the release last, one of each', () => {
    const stage = (id: string, kind: string) => ({ id, kind, gate: 'manual' })
    const valid = (steps: unknown[]) => pipelineSchema.safeParse(steps).success
    expect(valid([stage('context', 'context'), stage('ship', 'release')])).toBe(true)
    expect(valid([stage('check', 'manual'), stage('context', 'context')])).toBe(false)
    expect(valid([stage('context', 'context'), stage('more', 'context')])).toBe(false)
    expect(valid([stage('ship', 'release'), stage('check', 'manual')])).toBe(false)
    expect(valid([stage('ship', 'release'), stage('again', 'release')])).toBe(false)
  })

  it('takes ids of lower case words and digits joined by "-", up to 40 characters', () => {
    const id = (value: string) => stageIdSchema.safeParse(value).success
    expect(id('load-test')).toBe(true)
    expect(id('e2e')).toBe(true)
    expect(id('2fa-check')).toBe(true)
    const wrong = ['Load', 'load test', '-load', 'load-', 'load--test', 'lo_ad', '', 'a'.repeat(41)]
    for (const bad of wrong) {
      expect(id(bad), bad).toBe(false)
    }
  })

  it('takes only true or false for enabled', () => {
    const stage = { id: 'plan', kind: 'agent', gate: 'manual' }
    expect(pipelineSchema.safeParse([{ ...stage, enabled: true }]).success).toBe(true)
    expect(pipelineSchema.safeParse([{ ...stage, enabled: 'no' }]).success).toBe(false)
  })

  it('lets a manual stage run beside an agent stage', () => {
    const steps = [
      {
        id: 'build',
        parallel: [
          { id: 'implement', kind: 'agent', gate: 'auto' },
          { id: 'qa', kind: 'manual', gate: 'manual' },
        ],
      },
    ]
    expect(pipelineSchema.safeParse(steps).success).toBe(true)
  })

  it('takes a workflow on agent stages only, also inside a group', () => {
    const valid = (steps: unknown[]) => pipelineSchema.safeParse(steps).success
    expect(valid([{ id: 'plan', kind: 'agent', gate: 'manual', workflow: true }])).toBe(true)
    expect(
      valid([
        {
          id: 'build',
          parallel: [
            { id: 'a', kind: 'agent', gate: 'auto', workflow: true },
            { id: 'b', kind: 'manual', gate: 'manual' },
          ],
        },
      ]),
    ).toBe(true)
    for (const kind of ['context', 'review', 'testing', 'release', 'manual']) {
      const result = pipelineSchema.safeParse([{ id: 'x', kind, gate: 'manual', workflow: true }])
      expect(result.success).toBe(false)
      expect(result.error?.issues[0]?.message).toContain('workflow')
    }
    expect(valid([{ id: 'r', kind: 'review', gate: 'manual', workflow: false }])).toBe(true)
  })

  it('takes endShell on stages that start an agent or a command only', () => {
    for (const kind of ['context', 'agent', 'testing']) {
      const steps = [{ id: 'x', kind, gate: 'auto', endShell: true }]
      expect(pipelineSchema.safeParse(steps).success).toBe(true)
      expect(canEndShell({ kind: kind as Stage['kind'] })).toBe(true)
    }
    for (const kind of ['review', 'release', 'manual']) {
      const result = pipelineSchema.safeParse([{ id: 'x', kind, gate: 'manual', endShell: true }])
      expect(result.success).toBe(false)
      expect(result.error?.issues[0]?.message).toBe(
        `Only a stage that starts an agent or a command can end its shell, and "x" is a ${kind} stage`,
      )
      expect(canEndShell({ kind: kind as Stage['kind'] })).toBe(false)
    }
    const off = [{ id: 'r', kind: 'review', gate: 'manual', endShell: false }]
    expect(pipelineSchema.safeParse(off).success).toBe(true)
    const wrong = [{ id: 'plan', kind: 'agent', gate: 'manual', endShell: 'yes' }]
    expect(pipelineSchema.safeParse(wrong).success).toBe(false)
  })

  it('rejects a workflow that is not true or false', () => {
    for (const workflow of ['yes', 1, null]) {
      const steps = [{ id: 'plan', kind: 'agent', gate: 'manual', workflow }]
      expect(pipelineSchema.safeParse(steps).success).toBe(false)
    }
  })

  it('rejects a workflow on a manual stage inside a group, and names the first of several', () => {
    const inGroup = pipelineSchema.safeParse([
      {
        id: 'build',
        parallel: [
          { id: 'code', kind: 'agent', gate: 'auto', workflow: true },
          { id: 'sign-off', kind: 'manual', gate: 'manual', workflow: true },
        ],
      },
    ])
    expect(inGroup.error?.issues.map((issue) => issue.message)).toEqual([
      'Only agent stages can run with a workflow, and "sign-off" is a manual stage',
    ])

    const several = pipelineSchema.safeParse([
      { id: 'check', kind: 'manual', gate: 'manual', workflow: true },
      { id: 'review', kind: 'review', gate: 'manual', workflow: true },
    ])
    expect(several.error?.issues[0]?.message).toContain('"check"')
    expect(several.error?.issues).toHaveLength(2)
  })

  describe('the agent and arguments of a stage', () => {
    const own = { agent: 'codex', args: '--model gpt-5' }
    const build = (testSpec: object, implement: object = {}) => ({
      id: 'build',
      parallel: [
        { id: 'test-spec', kind: 'agent', gate: 'manual', output: 'test-spec.md', ...testSpec },
        { id: 'implement', kind: 'agent', gate: 'auto', ...implement },
      ],
    })
    const message = (id: string) =>
      `Only a stage outside Shell 1, or a custom stage that an agent works on, can have its own agent or arguments, and "${id}" cannot`

    it('takes them on a stage that runs beside the stage in Shell 1', () => {
      const parsed = pipelineSchema.parse([build(own)])
      expect(stagesOf(parsed[0] as Step)[0]).toMatchObject(own)
      expect(pipelineSchema.safeParse([build({ args: '--model x' })]).success).toBe(true)
    })

    it('rejects them on the stage in Shell 1, outside a group, and on a manual stage', () => {
      const issues = (steps: unknown[]) =>
        pipelineSchema.safeParse(steps).error?.issues.map((issue) => issue.message)
      expect(issues([build({}, own)])).toEqual([message('implement')])
      expect(issues([{ id: 'spec', kind: 'agent', gate: 'manual', ...own }])).toEqual([
        message('spec'),
      ])
      expect(
        issues([
          {
            id: 'build',
            parallel: [
              { id: 'implement', kind: 'agent', gate: 'auto' },
              { id: 'sign-off', kind: 'manual', gate: 'manual', agent: 'codex' },
            ],
          },
        ]),
      ).toEqual([message('sign-off')])
    })

    it('takes them on a stage that runs in its own shell outside a group', () => {
      const review = { id: 'agent-review', kind: 'agent', gate: 'auto', output: 'review.md' }
      expect(pipelineSchema.safeParse([{ ...review, ownShell: true, ...own }]).success).toBe(true)
      expect(pipelineSchema.safeParse([{ ...review, ...own }]).success).toBe(false)
    })

    it('rejects empty words', () => {
      expect(pipelineSchema.safeParse([build({ args: ' ' })]).success).toBe(false)
      expect(pipelineSchema.safeParse([build({ agent: '' })]).success).toBe(false)
    })

    it('gives them only to a stage outside Shell 1 when the task runs', () => {
      const [step] = pipelineSchema.parse([build(own, {})]) as Step[]
      const [testSpec, implement] = stagesOf(step as Step)
      expect(stageAgent(step as Step, testSpec as Stage)).toEqual(own)
      expect(stageAgent(step as Step, implement as Stage)).toEqual({})
    })

    it('takes them on a stage of your own that an agent works on, also where Shell 1 is', () => {
      const docs = { id: 'write-docs', kind: 'agent', gate: 'auto', ...own }
      const lint = { id: 'check-types', kind: 'testing', gate: 'auto', ...own }
      expect(pipelineSchema.safeParse([docs]).success).toBe(true)
      expect(pipelineSchema.safeParse([lint]).success).toBe(true)
      // The stage of a group that keeps Shell 1.
      const group = {
        id: 'build',
        parallel: [docs, { id: 'notes', kind: 'agent', gate: 'auto', output: 'notes.md' }],
      }
      expect(pipelineSchema.safeParse([group]).success).toBe(true)
    })

    it('rejects them on a stage of your own that no agent works on', () => {
      const messages = (kind: string) =>
        pipelineSchema
          .safeParse([{ id: 'my-step', kind, gate: 'manual', ...own }])
          .error?.issues.map((issue) => issue.message)
      for (const kind of ['context', 'review', 'release', 'manual']) {
        expect(messages(kind)).toEqual([message('my-step')])
      }
    })

    it('gives them to a stage of your own when the task runs', () => {
      const docs = { id: 'write-docs', kind: 'agent', gate: 'auto' } as const
      expect(stageAgent({ ...docs, ...own }, { ...docs, ...own })).toEqual(own)
      expect(stageAgent(docs, docs)).toEqual({})
      const spec = { id: 'spec', kind: 'agent', gate: 'manual', agent: 'codex' } as const
      expect(stageAgent(spec, spec)).toEqual({})
    })
  })

  it('rejects an empty pipeline and a bad stage id', () => {
    expect(pipelineSchema.safeParse([]).success).toBe(false)
    expect(pipelineSchema.safeParse([{ id: 'Spec', kind: 'agent', gate: 'auto' }]).success).toBe(
      false,
    )
  })
})

describe('stages that are off', () => {
  const stage = (id: string, kind: string, enabled?: boolean) => ({
    id,
    kind,
    gate: 'manual',
    ...(enabled === undefined ? {} : { enabled }),
  })
  const valid = (steps: unknown[]) => pipelineSchema.safeParse(steps).success

  it('leaves them out of the rules for the context and the release', () => {
    expect(valid([stage('check', 'manual', false), stage('context', 'context')])).toBe(true)
    expect(valid([stage('ship', 'release'), stage('check', 'manual', false)])).toBe(true)
    expect(valid([stage('context', 'context'), stage('more', 'context', false)])).toBe(true)
  })

  it('still needs unique ids, and one stage that is on', () => {
    expect(valid([stage('check', 'manual'), stage('check', 'manual', false)])).toBe(false)
    expect(valid([stage('check', 'manual', false)])).toBe(false)
  })

  it('needs two stages that are on in a group', () => {
    const group = (parallel: unknown[]) => [stage('first', 'manual'), { id: 'build', parallel }]
    expect(valid(group([stage('a', 'agent'), stage('b', 'agent', false)]))).toBe(false)
    expect(valid(group([stage('a', 'agent', false), stage('b', 'agent', false)]))).toBe(true)
    expect(
      valid(group([stage('a', 'agent'), stage('b', 'agent'), stage('c', 'agent', false)])),
    ).toBe(true)
  })

  it('leaves them out of the steps of a new task', () => {
    const steps = pipelineSchema.parse([
      stage('context', 'context'),
      stage('plan', 'agent', false),
      { id: 'build', parallel: [stage('a', 'agent', false), stage('b', 'agent', false)] },
      {
        id: 'checks',
        parallel: [stage('c', 'agent'), stage('d', 'agent'), stage('e', 'agent', false)],
      },
    ])
    expect(stepRefsOf(activeSteps(steps))).toEqual([
      'context',
      { id: 'checks', parallel: ['c', 'd'] },
    ])
  })
})

describe('the default pipeline', () => {
  it('runs the automated tests, then an agent review, and waits for no person', () => {
    expect(stepRefsOf(DEFAULT_PIPELINE)).toEqual([
      'context',
      'spec',
      'plan',
      { id: 'build', parallel: ['test-spec', 'implement'] },
      'testing',
      'agent-review',
      'release',
    ])
    expect(DEFAULT_PIPELINE[1]).toEqual({
      id: 'spec',
      kind: 'agent',
      gate: 'manual',
      output: 'spec.md',
      sections: ['Goal', 'Scope', 'Out of scope', 'Acceptance criteria', 'Code map'],
    })
    expect(activeSteps(DEFAULT_PIPELINE)).toEqual(DEFAULT_PIPELINE)
  })
})

describe('defaultPrompt', () => {
  it('asks the spec for a code map', () => {
    expect(defaultPrompt({ id: 'spec', kind: 'agent' })).toContain('Code map')
  })

  it('takes the prompt of the catalog', () => {
    expect(defaultPrompt({ id: 'plan', kind: 'agent' })).toContain('Write {dir}/plan.md')
    expect(defaultPrompt({ id: 'security', kind: 'agent' })).toContain('{dir}/security.md')
  })

  it('makes one for a stage of its own', () => {
    expect(defaultPrompt({ id: 'sign-off', kind: 'agent', output: 'sign-off.md' })).toBe(
      'Task: {title}\nDo the Sign off stage of this task.\nWrite {dir}/sign-off.md.',
    )
  })

  it('gives an older stage with a catalog id its own prompt when it does other work', () => {
    expect(defaultPrompt({ id: 'docs', kind: 'agent', output: 'docs.md' })).toBe(
      'Task: {title}\nDo the Documentation stage of this task.\nWrite {dir}/docs.md.',
    )
  })
})

describe('projectSettingsSchema', () => {
  it('needs at least one context source', () => {
    expect(projectSettingsSchema.safeParse({ context: { sources: [] } }).success).toBe(false)
  })

  it('fills every field of an empty file', () => {
    expect(projectSettingsSchema.parse({})).toEqual(DEFAULT_PROJECT_SETTINGS)
    expect(DEFAULT_PROJECT_SETTINGS.stages).toEqual(DEFAULT_PIPELINE)
    expect(DEFAULT_PROJECT_SETTINGS.tracker).toEqual({
      type: 'github',
      createIssue: 'manual',
      linkInPr: true,
    })
    expect(DEFAULT_PROJECT_SETTINGS.agents).toEqual({ permissionMode: 'auto' })
  })

  it('opens issues only when asked, unless the file says otherwise', () => {
    const createIssue = (tracker: unknown) =>
      projectSettingsSchema.parse({ tracker }).tracker.createIssue
    expect(createIssue({})).toBe('manual')
    expect(createIssue({ type: 'github' })).toBe('manual')
    for (const value of ['never', 'manual', 'on-spec-approved', 'on-pr']) {
      expect(createIssue({ createIssue: value })).toBe(value)
    }
    const always = projectSettingsSchema.safeParse({ tracker: { createIssue: 'always' } })
    expect(always.success).toBe(false)
  })

  it('takes auto or ask as the permission mode of the agents', () => {
    expect(projectSettingsSchema.parse({ agents: {} }).agents.permissionMode).toBe('auto')
    expect(
      projectSettingsSchema.parse({ agents: { permissionMode: 'ask' } }).agents.permissionMode,
    ).toBe('ask')
    const wrong = projectSettingsSchema.safeParse({ agents: { permissionMode: 'yolo' } })
    expect(wrong.success).toBe(false)
    expect(wrong.error?.issues[0]?.path).toEqual(['agents', 'permissionMode'])
  })

  it('reads context sources by type', () => {
    const settings = projectSettingsSchema.parse({
      context: {
        sources: [
          { type: 'mcp', name: 'Jira', prompt: 'Read {ref} with the jira MCP server' },
          { type: 'skill', name: 'Ticket', skill: 'fetch-ticket' },
        ],
      },
    })
    expect(settings.context.sources.map((source) => source.type)).toEqual(['mcp', 'skill'])
    expect(
      projectSettingsSchema.safeParse({ context: { sources: [{ type: 'mcp', name: 'Jira' }] } })
        .success,
    ).toBe(false)
  })
})

describe('resolveSteps', () => {
  it('turns the step ids of a task back into steps with their settings', () => {
    expect(resolveSteps(stepRefsOf(DEFAULT_PIPELINE), DEFAULT_PIPELINE)).toEqual(DEFAULT_PIPELINE)
  })

  it('finds a stage of a group for a task that has it on its own', () => {
    const [implement] = resolveSteps(LEGACY_STEP_REFS, DEFAULT_PIPELINE)
    expect(implement).toEqual({
      id: 'implement',
      kind: 'agent',
      gate: 'auto',
      workflow: true,
      ci: true,
    })
  })

  it('makes a stage of its own that is gone from the settings a manual one', () => {
    expect(resolveSteps(['sign-off'], DEFAULT_PIPELINE)).toEqual([
      { id: 'sign-off', kind: 'manual', gate: 'manual' },
    ])
  })

  it('keeps the catalog settings of a catalog stage that is gone from the settings', () => {
    const [security] = resolveSteps(['security'], DEFAULT_PIPELINE)
    expect(security).toMatchObject({ id: 'security', kind: 'agent', output: 'security.md' })
  })

  it('still runs a stage that is off for a task that has it', () => {
    const pipeline: Step[] = [{ id: 'plan', kind: 'agent', gate: 'auto', enabled: false }]
    const [plan] = resolveSteps(['plan'], pipeline)
    expect(plan).toMatchObject({ id: 'plan', gate: 'auto', enabled: false })
  })

  it('keeps the default settings of a default stage that is gone from the settings', () => {
    const [plan] = resolveSteps(['plan'], [{ id: 'implement', kind: 'agent', gate: 'auto' }])
    expect(plan).toMatchObject({ id: 'plan', kind: 'agent', output: 'plan.md' })
  })
})

describe('positions', () => {
  it('starts a task in draft at its first step', () => {
    expect(firstPosition(DEFAULT_PIPELINE)).toEqual({ stage: 'context', state: 'draft', lanes: {} })
  })

  it('starts a review stage waiting for you, and a group with a state per stage', () => {
    const review: Step = { id: 'review', kind: 'review', gate: 'manual' }
    const build = DEFAULT_PIPELINE[3]
    if (build === undefined) {
      throw new Error('The default pipeline changed')
    }
    expect(enterStep(review)).toEqual({ stage: 'review', state: 'review', lanes: {} })
    expect(enterStep(build)).toEqual({
      stage: 'build',
      state: 'ready',
      lanes: { 'test-spec': 'ready', implement: 'ready' },
    })
  })

  it('shows the most urgent lane of a group', () => {
    expect(combinedState({ a: 'running', b: 'review' })).toBe('review')
    expect(combinedState({ a: 'needs-input', b: 'failed' })).toBe('needs-input')
    expect(combinedState({ a: 'done', b: 'running' })).toBe('running')
    expect(combinedState({ a: 'done', b: 'done' })).toBe('done')
  })

  it('finds the step of a stage inside a group', () => {
    expect(stepIndex(DEFAULT_PIPELINE, 'implement')).toBe(3)
    expect(stepIndex(DEFAULT_PIPELINE, 'build')).toBe(3)
    expect(stepIndex(DEFAULT_PIPELINE, 'nope')).toBe(-1)
  })
})

describe('stageName', () => {
  it('uses the name of the catalog for a catalog stage', () => {
    expect(stageName({ id: 'clarify' })).toBe('Open questions')
    expect(stageName({ id: 'clarify', name: 'Questions' })).toBe('Questions')
  })

  it('names the testing and review stages by who does the work', () => {
    expect(stageName({ id: 'testing' })).toBe('Automated tests')
    expect(stageName({ id: 'qa' })).toBe('Manual testing')
    expect(stageName({ id: 'agent-review' })).toBe('Review')
    expect(stageName({ id: 'review' })).toBe('Manual review')
  })

  it('uses the name, else the id in words', () => {
    expect(stageName({ id: 'test-spec' })).toBe('Test spec')
    expect(stageName({ id: 'build', name: 'Test spec and Implement' })).toBe(
      'Test spec and Implement',
    )
  })
})

describe('terminals of a group', () => {
  it('gives the code stage the first terminal and the others the next ones', () => {
    const build = DEFAULT_PIPELINE[3]
    if (build === undefined) {
      throw new Error('The default pipeline changed')
    }
    expect(terminalOfStage([build], 'implement')).toBe('1')
    expect(terminalOfStage([build], 'test-spec')).toBe('2')
    expect(stageOfTerminal([build], build, '2')?.id).toBe('test-spec')
    expect(stageOfTerminal([build], build, '3')).toBeUndefined()
  })
})

describe('a stage with its own agent', () => {
  const own = (id: string, extra: object = {}): Stage => ({
    id,
    kind: 'agent',
    gate: 'auto',
    agent: 'copilot',
    ...extra,
  })
  // The pipeline of the issue: the task's agent does Spec and Implement in Shell 1.
  const steps: Step[] = [
    { id: 'spec', kind: 'agent', gate: 'manual', output: 'spec.md' },
    { ...own('test-spec', { output: 'test-spec.md', ownShell: true }), gate: 'manual' },
    { id: 'implement', kind: 'agent', gate: 'auto' },
    own('agent-review', { agent: 'cursor', output: 'review.md', ownShell: true }),
    own('ci-watch'),
    own('merge-pr'),
  ]

  it('gets a shell of its own from Shell 6, in pipeline order, and Shell 1 keeps the rest', () => {
    const shells = steps.map((step) => terminalOfStage(steps, step.id))
    expect(shells).toEqual(['1', '6', '1', '7', '8', '9'])
    expect(stageOfTerminal(steps, steps[4] as Step, '8')?.id).toBe('ci-watch')
    expect(stageOfTerminal(steps, steps[4] as Step, '1')).toBeUndefined()
    const position = { stage: 'ci-watch', state: 'running' as const, lanes: {} }
    expect(terminalLane(position, steps, '8')).toEqual({ stage: 'ci-watch', state: 'running' })
    // Shell 1 served Spec and Implement; the last of them names it.
    expect(terminalLane(position, steps, '1')).toEqual({ stage: 'implement', state: 'done' })
  })

  it('keeps the stage of a shell after its step is over', () => {
    const atImplement = { stage: 'implement', state: 'running' as const, lanes: {} }
    expect(terminalLane(atImplement, steps, '6')).toEqual({ stage: 'test-spec', state: 'done' })
    // A stage that has not run yet does not name a shell.
    expect(terminalLane(atImplement, steps, '7')).toEqual({ stage: null, state: null })
    expect(terminalLane(atImplement, steps, '2')).toEqual({ stage: null, state: null })
    const atSpec = { stage: 'spec', state: 'running' as const, lanes: {} }
    expect(terminalLane(atSpec, steps, '6')).toEqual({ stage: null, state: null })
    const finished = { stage: 'merge-pr', state: 'done' as const, lanes: {} }
    expect(terminalLane(finished, steps, '6')).toEqual({ stage: 'test-spec', state: 'done' })
  })

  it('gets one for arguments alone, and keeps Shell 1 or 5 without either', () => {
    const plain: Step[] = [
      { id: 'write-docs', kind: 'agent', gate: 'auto', args: '--model opus' },
      { id: 'notes', kind: 'agent', gate: 'auto' },
      { id: 'agent-review', kind: 'agent', gate: 'auto', output: 'r.md', ownShell: true },
    ]
    expect(plain.map((step) => terminalOfStage(plain, step.id))).toEqual(['6', '1', '5'])
  })

  it('gives a lane of a group its own shell, and the other lanes keep theirs', () => {
    const group: Step[] = [
      {
        id: 'build',
        parallel: [
          { id: 'implement', kind: 'agent', gate: 'auto' },
          own('test-spec', { output: 'test-spec.md' }),
          { id: 'notes', kind: 'agent', gate: 'auto', output: 'notes.md' },
          own('lint-notes', { output: 'lint.md' }),
        ],
      },
    ]
    const ids = ['implement', 'test-spec', 'notes', 'lint-notes']
    expect(ids.map((id) => terminalOfStage(group, id))).toEqual(['1', '6', '3', '7'])
    expect(stageOfTerminal(group, group[0] as Step, '7')?.id).toBe('lint-notes')
  })

  it('gives no shell to an agent the stage may not have', () => {
    const invalid: Step[] = [{ id: 'spec', kind: 'agent', gate: 'manual', agent: 'codex' }]
    expect(terminalOfStage(invalid, 'spec')).toBe('1')
  })
})

describe('a check on CI', () => {
  it('is offered on Implement and Automated tests of the default pipeline', () => {
    const offered = DEFAULT_PIPELINE.flatMap(stagesOf).filter(offersCi)
    expect(offered.map((stage) => stage.id)).toEqual(['implement', 'testing'])
  })

  it('is allowed only on a stage that changes code or runs tests', () => {
    const issues = (stage: object) =>
      pipelineSchema.safeParse([{ id: 'x', gate: 'auto', ci: true, ...stage }]).error?.issues
    expect(issues({ kind: 'agent' })).toBeUndefined()
    expect(issues({ kind: 'testing' })).toBeUndefined()
    for (const stage of [
      { kind: 'agent', output: 'notes.md' },
      { kind: 'review' },
      { kind: 'manual' },
    ]) {
      expect(issues(stage)?.[0]?.message).toBe(
        'Only a stage that changes code or runs tests can be checked on CI, and "x" cannot',
      )
    }
    expect(issues({ kind: 'review', ci: false })).toBeUndefined()
  })

  it('finds the last stage that changes code before a stage', () => {
    const target = (steps: Step[], id: string) => codeStageBefore(steps, id)?.id
    expect(target(DEFAULT_PIPELINE, 'testing')).toBe('implement')
    expect(target(DEFAULT_PIPELINE, 'implement')).toBeUndefined()
    const steps: Step[] = [
      { id: 'spec', kind: 'agent', gate: 'manual', output: 'spec.md' },
      {
        id: 'build',
        parallel: [
          { id: 'code', kind: 'agent', gate: 'auto' },
          { id: 'notes', kind: 'agent', gate: 'auto', output: 'notes.md' },
        ],
      },
      { id: 'testing', kind: 'testing', gate: 'auto', ci: true },
    ]
    expect(target(steps, 'testing')).toBe('code')
    expect(target(steps, 'spec')).toBeUndefined()
    expect(target(steps, 'missing')).toBeUndefined()
    // The last one wins; stages that write a file or wait for a person are skipped.
    const later: Step[] = [
      { id: 'implement', kind: 'agent', gate: 'auto' },
      { id: 'refactor', kind: 'agent', gate: 'auto' },
      { id: 'notes', kind: 'agent', gate: 'auto', output: 'notes.md' },
      { id: 'review', kind: 'review', gate: 'manual' },
      { id: 'testing', kind: 'testing', gate: 'auto', ci: true },
    ]
    expect(target(later, 'testing')).toBe('refactor')
  })
})

describe('a stage in its own shell', () => {
  const review: Step = {
    id: 'agent-review',
    kind: 'agent',
    gate: 'auto',
    output: 'review.md',
    ownShell: true,
  }

  it('runs in Shell 5, apart from the agent in Shell 1 and the stages of a group', () => {
    expect(terminalOfStage([review], 'agent-review')).toBe('5')
    expect(stageOfTerminal([review], review, '5')?.id).toBe('agent-review')
    expect(stageOfTerminal([review], review, '1')).toBeUndefined()
    const position = { stage: 'agent-review', state: 'running' as const, lanes: {} }
    expect(terminalLane(position, [review], '5')).toEqual({
      stage: 'agent-review',
      state: 'running',
    })
    expect(terminalLane(position, [review], '1')).toEqual({ stage: null, state: null })
    expect(stageAgent(review, { ...review, agent: 'codex' } as Stage)).toEqual({ agent: 'codex' })
  })

  it('is only for an agent stage that writes a report', () => {
    const message = (id: string) =>
      `Only an agent stage with an output file can run in its own shell, and "${id}" cannot`
    const issues = (stage: object) =>
      pipelineSchema.safeParse([{ id: 'x', gate: 'auto', ownShell: true, ...stage }]).error?.issues
    expect(issues({ kind: 'agent', output: 'x.md' })).toBeUndefined()
    expect(issues({ kind: 'agent' })?.[0]?.message).toBe(message('x'))
    expect(issues({ kind: 'review' })?.[0]?.message).toBe(message('x'))
  })
})

describe('a stage that ends its shell', () => {
  const ends = (steps: readonly Step[], id: string) => {
    const stage = steps.flatMap(stagesOf).find((each) => each.id === id)
    if (stage === undefined) {
      throw new Error(`No stage ${id}`)
    }
    return endsShell1(steps, stage)
  }

  it('would end Shell 1 for the stages of the default pipeline that run there', () => {
    const ids = ['context', 'spec', 'plan', 'implement', 'test-spec', 'testing', 'agent-review']
    expect(ids.map((id) => ends(DEFAULT_PIPELINE, id))).toEqual([
      true,
      true,
      true,
      true,
      false,
      true,
      false,
    ])
  })

  it('ends a shell of its own for a testing stage with a command, or a stage with its own agent', () => {
    const steps: Step[] = [
      { id: 'testing', kind: 'testing', gate: 'auto', command: 'pnpm test' },
      { id: 'write-docs', kind: 'agent', gate: 'auto', args: '--model opus' },
      { id: 'notes', kind: 'agent', gate: 'auto' },
    ]
    expect(steps.map((step) => ends(steps, step.id))).toEqual([false, false, true])
  })

  it('would end Shell 1 for a stage in a path of a branch', () => {
    const implement: Stage = { id: 'implement', kind: 'agent', gate: 'auto' }
    const docs: Stage = { id: 'docs', kind: 'agent', gate: 'auto' }
    const steps: Step[] = [
      {
        id: 'after-plan',
        paths: [
          { id: 'code', when: { type: 'command', run: 'true' }, stages: [implement] },
          { id: 'text', stages: [docs] },
        ],
      },
    ]
    expect(ends(steps, 'implement')).toBe(true)
  })
})

describe('terminalLane', () => {
  const build = DEFAULT_PIPELINE[3]
  const spec = DEFAULT_PIPELINE[1]
  if (build === undefined || spec === undefined) {
    throw new Error('The default pipeline changed')
  }
  const inBuild = (lanes: Record<string, TaskState>) => ({
    stage: build.id,
    state: combinedState(lanes),
    lanes,
  })

  it('gives each terminal of a group the state of its own lane', () => {
    const position = inBuild({ implement: 'running', 'test-spec': 'review' })
    expect(position.state).toBe('review')
    expect(terminalLane(position, [build], '1')).toEqual({ stage: 'implement', state: 'running' })
    expect(terminalLane(position, [build], '2')).toEqual({ stage: 'test-spec', state: 'review' })
  })

  it('keeps the lanes apart when one needs input', () => {
    const position = inBuild({ implement: 'running', 'test-spec': 'needs-input' })
    expect(position.state).toBe('needs-input')
    expect(terminalLane(position, [build], '1').state).toBe('running')
    expect(terminalLane(position, [build], '2').state).toBe('needs-input')
  })

  it('shows a finished lane as done, also when it is gone from the lanes', () => {
    const finished = inBuild({ implement: 'running', 'test-spec': 'done' })
    expect(terminalLane(finished, [build], '2')).toEqual({ stage: 'test-spec', state: 'done' })
    expect(terminalLane({ stage: build.id, state: 'running', lanes: {} }, [build], '2')).toEqual({
      stage: 'test-spec',
      state: 'done',
    })
  })

  it('gives no lane to a terminal the group does not use', () => {
    const position = inBuild({ implement: 'running', 'test-spec': 'review' })
    expect(terminalLane(position, [build], '3')).toEqual({ stage: null, state: null })
  })

  it('shows the shell of a stage before the group as done', () => {
    const ownSpec: Step = {
      id: 'spec',
      kind: 'agent',
      gate: 'manual',
      output: 'spec.md',
      ownShell: true,
    }
    const position = inBuild({ implement: 'running', 'test-spec': 'review' })
    expect(terminalLane(position, [ownSpec, build], '5')).toEqual({ stage: 'spec', state: 'done' })
  })

  it('shows the shell of a group stage as done once the task is past the group', () => {
    const position = { stage: 'testing', state: 'running' as const, lanes: {} }
    expect(terminalLane(position, DEFAULT_PIPELINE, '2')).toEqual({
      stage: 'test-spec',
      state: 'done',
    })
    // Shell 1 is the shell of the current stage too.
    expect(terminalLane(position, DEFAULT_PIPELINE, '1')).toEqual({
      stage: 'testing',
      state: 'running',
    })
  })

  it('gives the task state to Shell 1 outside a group, and no lane to the others', () => {
    const position = { stage: spec.id, state: 'running' as const, lanes: {} }
    expect(terminalLane(position, [spec], '1')).toEqual({ stage: 'spec', state: 'running' })
    expect(terminalLane(position, [spec], '2')).toEqual({ stage: null, state: null })
  })

  it('uses the stage of the task when its step is unknown', () => {
    const position = { stage: 'gone', state: 'review' as const, lanes: {} }
    expect(terminalLane(position, [], '1')).toEqual({ stage: 'gone', state: 'review' })
    expect(terminalLane(position, [], '2')).toEqual({ stage: null, state: null })
  })
})

describe('workflows', () => {
  const stage = (id: string) => DEFAULT_PIPELINE.flatMap(stagesOf).find((each) => each.id === id)

  it('offers a workflow on plan and implement of the default pipeline only', () => {
    const offered = DEFAULT_PIPELINE.flatMap(stagesOf)
      .filter(offersWorkflow)
      .map((each) => each.id)
    expect(offered).toEqual(['plan', 'implement'])
  })

  it('offers a workflow on a stage of your own that asks for it, and not without the field', () => {
    expect(offersWorkflow({ kind: 'agent', workflow: true })).toBe(true)
    expect(offersWorkflow({ kind: 'agent' })).toBe(false)
    expect(offersWorkflow({ kind: 'agent', workflow: false })).toBe(false)
    expect(offersWorkflow({ kind: 'review', workflow: true })).toBe(false)
  })

  it('lets only Claude Code run a workflow; a task with no agent gets Claude Code', () => {
    expect(canRunWorkflow('claude-code')).toBe(true)
    expect(canRunWorkflow(null)).toBe(true)
    for (const agent of ['codex', 'gemini', 'copilot', 'cursor']) {
      expect(canRunWorkflow(agent)).toBe(false)
    }
  })

  it('runs a workflow when the stage offers it, the task chose it and the agent can', () => {
    const implement = stage('implement')
    if (implement === undefined) {
      throw new Error('The default pipeline has no implement stage')
    }
    expect(runsWorkflow(implement, ['implement'], 'claude-code')).toBe(true)
    expect(runsWorkflow(implement, [], 'claude-code')).toBe(false)
    expect(runsWorkflow(implement, ['plan'], 'claude-code')).toBe(false)
    expect(runsWorkflow(implement, ['implement'], 'codex')).toBe(false)
    expect(runsWorkflow({ ...implement, workflow: false }, ['implement'], 'claude-code')).toBe(
      false,
    )
  })
})

describe('a branch', () => {
  const plan: Stage = { id: 'plan', kind: 'agent', gate: 'manual', output: 'plan.md' }
  const implement: Stage = { id: 'implement', kind: 'agent', gate: 'auto' }
  const testing: Stage = { id: 'testing', kind: 'testing', gate: 'auto' }
  const review: Stage = { id: 'review', kind: 'review', gate: 'manual' }
  const release: Stage = { id: 'release', kind: 'release', gate: 'manual' }
  const small: BranchPath = {
    id: 'small',
    name: 'Small change',
    when: { type: 'output', file: 'plan.md', section: 'Size', contains: 'small' },
    stages: [implement, testing],
  }
  const prReview: BranchPath = {
    id: 'pr-review',
    name: 'Review PR',
    when: { type: 'command', run: 'test -f .no-review' },
    not: true,
    stages: [review],
  }
  const branch: StageBranch = { id: 'after-plan', name: 'After plan', paths: [small, prReview] }
  const steps: Step[] = [plan, branch, release]

  const messages = (value: unknown[]) =>
    pipelineSchema.safeParse(value).error?.issues.map((issue) => issue.message)
  const withPaths = (paths: unknown[]) => [plan, { id: 'after-plan', paths }, release]
  const path = (id: string, stages: unknown[], extra: object = {}) => ({
    id,
    when: { type: 'command', run: 'true' },
    stages,
    ...extra,
  })
  const otherwise = (id: string, stages: unknown[]) => ({ id, stages })
  const off = (stage: Stage): Stage => ({ ...stage, enabled: false })

  it('reads a branch and gives it back the same, as neither a stage nor a group', () => {
    const parsed = pipelineSchema.parse(steps)
    expect(parsed).toEqual(steps)
    expect(isBranch(parsed[1] as Step)).toBe(true)
    expect(isGroup(branch)).toBe(false)
    expect(isStage(branch)).toBe(false)
    expect(isBranch(plan)).toBe(false)
    expect(isStage(plan)).toBe(true)
    // No names, no section, and an Otherwise path last.
    const plain = withPaths([
      path('a', [implement], { when: { type: 'output', file: 'plan.md', contains: 'small' } }),
      otherwise('b', [review]),
    ])
    expect(pipelineSchema.parse(plain)).toEqual(plain)
  })

  it('holds 2 to 4 paths', () => {
    const paths = ['a', 'b', 'c', 'd', 'e'].map((id) =>
      path(id, [{ id: `stage-${id}`, kind: 'agent', gate: 'auto' }]),
    )
    expect(messages(withPaths(paths.slice(0, 1)))).toEqual(['A branch holds 2 to 4 paths'])
    expect(messages(withPaths(paths.slice(0, 2)))).toBeUndefined()
    expect(messages(withPaths(paths.slice(0, 4)))).toBeUndefined()
    expect(messages(withPaths(paths))).toEqual(['A branch holds 2 to 4 paths'])
  })

  it('needs ids that are unique across stages, paths and branches', () => {
    expect(messages(withPaths([path('a', [implement]), path('a', [review])]))).toEqual([
      'The path id "a" is used twice',
    ])
    expect(messages(withPaths([path('plan', [implement]), path('b', [review])]))).toEqual([
      'The path id "plan" is used twice',
    ])
    expect(messages(withPaths([path('a', [implement]), path('b', [implement])]))).toEqual([
      'The stage id "implement" is used twice',
    ])
    expect(messages(withPaths([path('a', [plan]), path('b', [review])]))).toEqual([
      'The stage id "plan" is used twice',
    ])
  })

  it('takes the Otherwise path, with no condition, only last', () => {
    const message = 'Only the last path can be the Otherwise path'
    expect(messages(withPaths([otherwise('a', [implement]), path('b', [review])]))).toEqual([
      message,
    ])
    expect(messages(withPaths([otherwise('a', [implement]), otherwise('b', [review])]))).toEqual([
      message,
    ])
    expect(messages(withPaths([path('a', [implement]), otherwise('b', [review])]))).toBeUndefined()
  })

  it('needs a stage in each path, and single stages only', () => {
    expect(messages(withPaths([path('a', []), path('b', [review])]))).toEqual([
      'A path has at least one stage',
    ])
    const group = { id: 'build', parallel: [implement, { ...testing, kind: 'agent' }] }
    const valid = (paths: unknown[]) => pipelineSchema.safeParse(withPaths(paths)).success
    expect(valid([path('a', [group]), path('b', [review])])).toBe(false)
    const nested = { id: 'inner', paths: [path('x', [implement]), path('y', [testing])] }
    expect(valid([path('a', [nested]), path('b', [review])])).toBe(false)
  })

  it('takes no context or release stage in a path', () => {
    const context = { id: 'more', kind: 'context', gate: 'auto' }
    expect(messages(withPaths([path('a', [context]), path('b', [review])]))).toEqual([
      '"more" is a context stage, which cannot be in a path',
    ])
    const ship = { id: 'ship', kind: 'release', gate: 'manual' }
    expect(messages(withPaths([path('a', [implement, ship]), path('b', [review])]))).toEqual([
      '"ship" is a release stage, which cannot be in a path',
    ])
  })

  it('cannot be the first step or come right after a branch, but may be the last', () => {
    expect(messages([branch, release])).toEqual(['A branch cannot be the first step'])
    expect(messages([off(plan), branch, release])).toEqual(['A branch cannot be the first step'])
    expect(messages([plan, branch])).toBeUndefined()
    const next = {
      id: 'after-build',
      paths: [
        path('docs', [{ ...plan, id: 'write-docs' }]),
        otherwise('none', [{ ...review, id: 'qa' }]),
      ],
    }
    // A skipped branch chooses no path for the next one, so a stage must end between them.
    const message = 'A branch cannot come right after a branch. Put a stage between them'
    expect(messages([plan, branch, next, release])).toEqual([message])
    const between: Stage = { id: 'docs-check', kind: 'agent', gate: 'auto' }
    expect(messages([plan, branch, off(between), next, release])).toEqual([message])
    expect(messages([plan, branch, between, next, release])).toBeUndefined()
    // A branch left with only its Otherwise path runs it as plain steps.
    const lone = {
      ...next,
      paths: [path('docs', [off({ ...plan, id: 'write-docs' })]), next.paths[1]],
    }
    expect(messages([plan, branch, lone, release])).toBeUndefined()
  })

  it('checks the fields of a condition', () => {
    const when = (condition: object) =>
      messages(withPaths([path('a', [implement], { when: condition }), path('b', [review])]))
    expect(when({ type: 'output', file: 'plan.md', contains: 'small' })).toBeUndefined()
    for (const file of ['plan.txt', '', '../x.md']) {
      expect(when({ type: 'output', file, contains: 'small' })).toEqual([
        'A condition file is a markdown file, such as plan.md',
      ])
    }
    expect(when({ type: 'output', file: 'plan.md', contains: '  ' })).toEqual([
      'Give the text to look for',
    ])
    expect(when({ type: 'command', run: ' ' })).toEqual(['Give the command to run'])
    expect(when({ type: 'label', has: 'x' })).toBeDefined()
  })

  it('leaves out stages and paths that are off, and runs a lone Otherwise path as steps', () => {
    const docs: Stage = { id: 'write-docs', kind: 'agent', gate: 'auto' }
    const when = small.when
    const three: StageBranch = {
      id: 'after-plan',
      paths: [
        { id: 'a', when, stages: [implement, off(testing)] },
        { id: 'b', when, stages: [off(review)] },
        { id: 'c', stages: [docs] },
      ],
    }
    expect(activeSteps([plan, three, release])).toEqual([
      plan,
      {
        id: 'after-plan',
        paths: [
          { id: 'a', when, stages: [implement] },
          { id: 'c', stages: [docs] },
        ],
      },
      release,
    ])
    const lone: StageBranch = {
      id: 'after-plan',
      paths: [
        { id: 'a', when, stages: [off(implement)] },
        { id: 'b', stages: [review, testing] },
      ],
    }
    expect(activeSteps([plan, lone, release])).toEqual([plan, review, testing, release])
    expect(messages([plan, lone, release])).toBeUndefined()
    const none: StageBranch = {
      id: 'after-plan',
      paths: [
        { id: 'a', when, stages: [off(review)] },
        { id: 'b', stages: [off(testing)] },
      ],
    }
    expect(activeSteps([plan, none, release])).toEqual([plan, release])
    expect(messages([plan, none, release])).toBeUndefined()
  })

  it('needs another path that is on when the one left has a condition', () => {
    expect(messages(withPaths([path('a', [implement]), path('b', [off(review)])]))).toEqual([
      'Turn on a stage in another path, or remove the branch',
    ])
  })

  it('finds the stages of its paths, and the path of a stage', () => {
    expect(stagesOf(branch).map((stage) => stage.id)).toEqual(['implement', 'testing', 'review'])
    expect(findStep(steps, 'review')).toBe(branch)
    expect(stepIndex(steps, 'testing')).toBe(1)
    expect(stepIndex(steps, 'after-plan')).toBe(1)
    expect(pathOf(steps, 'testing')).toEqual({ branch, path: small, index: 1 })
    expect(pathOf(steps, 'review')).toEqual({ branch, path: prReview, index: 0 })
    expect(pathOf(steps, 'plan')).toBeUndefined()
    expect(pathOf(steps, 'nope')).toBeUndefined()
  })

  it('keeps the ids of its paths in a task, and reads the rest from the settings', () => {
    const refs = stepRefsOf(steps)
    expect(refs).toEqual([
      'plan',
      {
        id: 'after-plan',
        paths: [
          { id: 'small', stages: ['implement', 'testing'] },
          { id: 'pr-review', stages: ['review'] },
        ],
      },
      'release',
    ])
    expect(refs.every((ref) => stepRefSchema.safeParse(ref).success)).toBe(true)
    expect(
      stepRefSchema.safeParse({ id: 'b', paths: [{ id: 'a', stages: ['implement'] }] }).success,
    ).toBe(false)
    expect(resolveSteps(refs, steps)).toEqual(steps)

    // The settings changed: a new condition counts, and a path that is gone keeps its stages, with
    // the catalog settings of a stage that is gone too.
    const now: Step[] = [
      plan,
      {
        id: 'after-plan',
        paths: [
          { ...small, not: true },
          { id: 'other', stages: [testing] },
        ],
      },
      release,
    ]
    expect(resolveSteps(refs, now)[1]).toEqual({
      id: 'after-plan',
      paths: [
        { ...small, not: true },
        { id: 'pr-review', stages: [{ ...review, name: 'Manual review' }] },
      ],
    })
  })

  it('says its condition in words', () => {
    expect(describeCondition(small)).toBe('plan.md "Size" contains "small"')
    const file = { type: 'output', file: 'plan.md', contains: 'small' } as const
    expect(describeCondition({ when: file })).toBe('plan.md contains "small"')
    expect(describeCondition({ when: file, not: true })).toBe('plan.md does not contain "small"')
    expect(describeCondition({ when: { type: 'command', run: 'test -f x' } })).toBe(
      'test -f x succeeds',
    )
    expect(describeCondition(prReview)).toBe('test -f .no-review does not succeed')
    expect(describeCondition({})).toBe('Otherwise')
    expect(describeCondition({ not: true })).toBe('Otherwise')
  })

  it('is entered at the first stage of the path taken', () => {
    expect(enterStep(branch, undefined, 'pr-review')).toEqual({
      stage: 'review',
      state: 'review',
      lanes: {},
    })
    expect(enterStep(branch, 'draft', 'small')).toEqual({
      stage: 'implement',
      state: 'draft',
      lanes: {},
    })
  })

  it('tells the stages of a path the task did not take', () => {
    expect(isNotTaken(steps, { 'after-plan': 'small' }, 'review')).toBe(true)
    expect(isNotTaken(steps, { 'after-plan': 'small' }, 'implement')).toBe(false)
    expect(isNotTaken(steps, { 'after-plan': '' }, 'implement')).toBe(true)
    expect(isNotTaken(steps, { 'after-plan': 'small' }, 'plan')).toBe(false)
    expect(isNotTaken(steps, {}, 'review')).toBe(false)
    expect(isNotTaken(steps, undefined, 'review')).toBe(false)
  })

  it('finds the stage that changes code only on the way the task went', () => {
    const loadTest: Stage = { id: 'load-test', kind: 'testing', gate: 'auto', ci: true }
    const all: Step[] = [plan, branch, loadTest]
    expect(codeStageBefore(all, 'testing')?.id).toBe('implement')
    expect(codeStageBefore(all, 'implement')).toBeUndefined()
    expect(codeStageBefore(all, 'load-test')?.id).toBe('implement')
    expect(codeStageBefore(all, 'load-test', { 'after-plan': 'small' })?.id).toBe('implement')
    expect(codeStageBefore(all, 'load-test', { 'after-plan': 'pr-review' })).toBeUndefined()
    expect(codeStageBefore(all, 'load-test', { 'after-plan': '' })).toBeUndefined()
  })

  it('gives a stage in a path the shell of a single stage', () => {
    const agentReview: Stage = {
      id: 'agent-review',
      kind: 'agent',
      gate: 'auto',
      output: 'review.md',
      ownShell: true,
    }
    const docs: Stage = { id: 'write-docs', kind: 'agent', gate: 'auto', agent: 'codex' }
    const notes: Stage = {
      id: 'notes',
      kind: 'agent',
      gate: 'auto',
      output: 'notes.md',
      agent: 'cursor',
      ownShell: true,
    }
    const shells: Step[] = [
      plan,
      {
        id: 'after-plan',
        paths: [
          { id: 'a', when: small.when, stages: [implement, agentReview] },
          { id: 'b', stages: [docs, notes] },
        ],
      },
    ]
    const ids = ['plan', 'implement', 'agent-review', 'write-docs', 'notes']
    expect(ids.map((id) => terminalOfStage(shells, id))).toEqual(['1', '1', '5', '6', '7'])
    expect(pipelineSchema.safeParse(shells).success).toBe(true)

    const atReview = {
      stage: 'agent-review',
      state: 'running' as const,
      lanes: {},
      paths: { 'after-plan': 'a' },
    }
    expect(terminalLane(atReview, shells, '5')).toEqual({ stage: 'agent-review', state: 'running' })
    expect(terminalLane(atReview, shells, '1')).toEqual({ stage: 'implement', state: 'done' })
    const atNotes = {
      stage: 'notes',
      state: 'running' as const,
      lanes: {},
      paths: { 'after-plan': 'b' },
    }
    expect(terminalLane(atNotes, shells, '7')).toEqual({ stage: 'notes', state: 'running' })
    // Implement is on the path the task did not take.
    expect(terminalLane(atNotes, shells, '1')).toEqual({ stage: 'plan', state: 'done' })
    expect(terminalLane(atNotes, shells, '6')).toEqual({ stage: 'write-docs', state: 'done' })
  })
})
