import { describe, expect, it } from 'vitest'
import { DEFAULT_PROJECT_SETTINGS, projectSettingsSchema } from '../../projects/project-settings.js'
import { canEndShell } from '../capabilities.js'
import { DEFAULT_PIPELINE } from '../default-pipeline.js'
import { pipelineSchema } from '../pipeline-schema.js'
import {
  type BranchPath,
  isBranch,
  isGroup,
  isStage,
  type Stage,
  type StageBranch,
  type Step,
  stageIdSchema,
  stagesOf,
} from '../schema.js'
import { stageAgent } from '../terminal-lane.js'

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
        `Only a stage that starts an agent or a command can end its terminal, and "x" is a ${kind} stage`,
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
      `Only a stage outside Terminal 1, or a custom stage that an agent works on, can have its own agent or arguments, and "${id}" cannot`

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

  it('has no send-back limit unless the file sets a whole number of 0 or more', () => {
    const most = (agents: unknown) => projectSettingsSchema.parse({ agents }).agents.maxSendBacks
    expect(most({})).toBeUndefined()
    expect(most({ maxSendBacks: 0 })).toBe(0)
    expect(most({ maxSendBacks: 3 })).toBe(3)
    for (const value of [-1, 1.5, '2', null]) {
      const wrong = projectSettingsSchema.safeParse({ agents: { maxSendBacks: value } })
      expect(wrong.success).toBe(false)
      expect(wrong.error?.issues[0]?.path).toEqual(['agents', 'maxSendBacks'])
    }
  })

  it('takes 0 or 5 to 1440 minutes before an idle agent pauses', () => {
    const minutes = (agents: unknown) =>
      projectSettingsSchema.parse({ agents }).agents.pauseIdleAfterMinutes
    expect(minutes({})).toBeUndefined()
    for (const value of [0, 5, 30, 1440]) {
      expect(minutes({ pauseIdleAfterMinutes: value })).toBe(value)
    }
    for (const value of [1, 4, 1441, -1, 2.5, '30']) {
      const wrong = projectSettingsSchema.safeParse({ agents: { pauseIdleAfterMinutes: value } })
      expect(wrong.success).toBe(false)
      expect(wrong.error?.issues[0]?.path).toEqual(['agents', 'pauseIdleAfterMinutes'])
    }
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

  it('needs another path that is on when the one left has a condition', () => {
    expect(messages(withPaths([path('a', [implement]), path('b', [off(review)])]))).toEqual([
      'Turn on a stage in another path, or remove the branch',
    ])
  })
})
