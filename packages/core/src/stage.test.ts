import { describe, expect, it } from 'vitest'
import { DEFAULT_PROJECT_SETTINGS, projectSettingsSchema } from './project-settings.js'
import {
  combinedState,
  DEFAULT_PIPELINE,
  enterStep,
  firstPosition,
  LEGACY_STEP_REFS,
  pipelineSchema,
  resolveSteps,
  stageName,
  stageOfTerminal,
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

  it('rejects an empty pipeline and a bad stage id', () => {
    expect(pipelineSchema.safeParse([]).success).toBe(false)
    expect(pipelineSchema.safeParse([{ id: 'Spec', kind: 'agent', gate: 'auto' }]).success).toBe(
      false,
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
    expect(implement).toEqual({ id: 'implement', kind: 'agent', gate: 'auto' })
  })

  it('makes a stage of its own that is gone from the settings a manual one', () => {
    expect(resolveSteps(['security'], DEFAULT_PIPELINE)).toEqual([
      { id: 'security', kind: 'manual', gate: 'manual' },
    ])
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
    const review = DEFAULT_PIPELINE[4]
    const build = DEFAULT_PIPELINE[3]
    if (review === undefined || build === undefined) {
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
