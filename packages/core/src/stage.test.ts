import { describe, expect, it } from 'vitest'
import { DEFAULT_PROJECT_SETTINGS, projectSettingsSchema } from './project-settings.js'
import {
  activeSteps,
  canRunWorkflow,
  combinedState,
  DEFAULT_PIPELINE,
  defaultPrompt,
  enterStep,
  firstPosition,
  LEGACY_STEP_REFS,
  offersCi,
  offersWorkflow,
  pipelineSchema,
  resolveSteps,
  runsWorkflow,
  type Stage,
  type Step,
  stageAgent,
  stageIdSchema,
  stageName,
  stageOfTerminal,
  stagesOf,
  stepIndex,
  stepRefsOf,
  terminalLane,
  terminalOfStage,
} from './stage.js'
import type { TaskState } from './task-state.js'

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
    expect(pipelineSchema.safeParse(group([agent('a')])).success).toBe(false)
    expect(pipelineSchema.safeParse(group([agent('a'), agent('b')])).success).toBe(true)
    expect(pipelineSchema.safeParse(group(['a', 'b', 'c', 'd', 'e'].map(agent))).success).toBe(
      false,
    )
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
      `Only a stage that runs in its own shell can have its own agent or arguments, and "${id}" cannot`

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
      createIssue: 'on-spec-approved',
      linkInPr: true,
    })
    expect(DEFAULT_PROJECT_SETTINGS.agents).toEqual({ permissionMode: 'auto' })
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
    expect(terminalOfStage(build, 'implement')).toBe('1')
    expect(terminalOfStage(build, 'test-spec')).toBe('2')
    expect(stageOfTerminal(build, '2')?.id).toBe('test-spec')
    expect(stageOfTerminal(build, '3')).toBeUndefined()
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
    expect(terminalOfStage(review, 'agent-review')).toBe('5')
    expect(stageOfTerminal(review, '5')?.id).toBe('agent-review')
    expect(stageOfTerminal(review, '1')).toBeUndefined()
    const position = { stage: 'agent-review', state: 'running' as const, lanes: {} }
    expect(terminalLane(position, review, '5')).toEqual({ stage: 'agent-review', state: 'running' })
    expect(terminalLane(position, review, '1')).toEqual({ stage: null, state: null })
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
    expect(terminalLane(position, build, '1')).toEqual({ stage: 'implement', state: 'running' })
    expect(terminalLane(position, build, '2')).toEqual({ stage: 'test-spec', state: 'review' })
  })

  it('keeps the lanes apart when one needs input', () => {
    const position = inBuild({ implement: 'running', 'test-spec': 'needs-input' })
    expect(position.state).toBe('needs-input')
    expect(terminalLane(position, build, '1').state).toBe('running')
    expect(terminalLane(position, build, '2').state).toBe('needs-input')
  })

  it('shows a finished lane as done, also when it is gone from the lanes', () => {
    const finished = inBuild({ implement: 'running', 'test-spec': 'done' })
    expect(terminalLane(finished, build, '2')).toEqual({ stage: 'test-spec', state: 'done' })
    expect(terminalLane({ stage: build.id, state: 'running', lanes: {} }, build, '2')).toEqual({
      stage: 'test-spec',
      state: 'done',
    })
  })

  it('gives no lane to a terminal the group does not use', () => {
    const position = inBuild({ implement: 'running', 'test-spec': 'review' })
    expect(terminalLane(position, build, '3')).toEqual({ stage: null, state: null })
  })

  it('gives the task state to Shell 1 outside a group, and no lane to the others', () => {
    const position = { stage: spec.id, state: 'running' as const, lanes: {} }
    expect(terminalLane(position, spec, '1')).toEqual({ stage: 'spec', state: 'running' })
    expect(terminalLane(position, spec, '2')).toEqual({ stage: null, state: null })
  })

  it('uses the stage of the task when its step is unknown', () => {
    const position = { stage: 'gone', state: 'review' as const, lanes: {} }
    expect(terminalLane(position, undefined, '1')).toEqual({ stage: 'gone', state: 'review' })
    expect(terminalLane(position, undefined, '2')).toEqual({ stage: null, state: null })
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
