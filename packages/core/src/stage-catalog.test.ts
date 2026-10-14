import { describe, expect, it } from 'vitest'
import { pipelineIssues } from './pipeline-issues.js'
import { DEFAULT_PIPELINE, defaultPrompt, GROUP_KINDS, type Step, stageSchema } from './stage.js'
import { catalogEntry, STAGE_CATALOG, STAGE_PHASES } from './stage-catalog.js'

describe('STAGE_CATALOG', () => {
  it('holds the 28 stages of the spec, by phase', () => {
    const byPhase = Object.fromEntries(
      STAGE_PHASES.map((phase) => [
        phase,
        STAGE_CATALOG.filter((entry) => entry.phase === phase).map((entry) => entry.stage.id),
      ]),
    )
    expect(byPhase).toEqual({
      understand: ['context', 'research', 'clarify'],
      design: ['spec', 'ux-design', 'architecture', 'threat-model', 'plan'],
      build: ['test-spec', 'implement', 'migration', 'refactor', 'docs', 'i18n'],
      check: [
        'lint',
        'self-review',
        'security',
        'performance',
        'accessibility',
        'agent-review',
        'review',
        'testing',
        'e2e',
        'qa',
        'design-review',
      ],
      ship: ['changelog', 'staging', 'release'],
    })
  })

  it('gives each stage the kind, gate and output of the spec', () => {
    const shape = (id: string) => {
      const stage = catalogEntry(id)?.stage
      return [stage?.kind, stage?.gate, stage?.output]
    }
    expect(shape('research')).toEqual(['agent', 'manual', 'research.md'])
    expect(shape('clarify')).toEqual(['agent', 'manual', 'questions.md'])
    expect(shape('ux-design')).toEqual(['agent', 'manual', 'ux.md'])
    expect(shape('migration')).toEqual(['agent', 'manual', undefined])
    expect(shape('refactor')).toEqual(['agent', 'auto', undefined])
    expect(shape('lint')).toEqual(['testing', 'auto', undefined])
    expect(shape('self-review')).toEqual(['agent', 'auto', 'self-review.md'])
    expect(shape('qa')).toEqual(['manual', 'manual', undefined])
    expect(shape('staging')).toEqual(['manual', 'manual', undefined])
    expect(catalogEntry('lint')?.needsCommand).toBe(true)
    expect(catalogEntry('e2e')?.needsCommand).toBe(true)
  })

  it('gives every agent and testing stage a prompt', () => {
    for (const { stage } of STAGE_CATALOG) {
      if (stage.kind === 'agent' || stage.kind === 'testing') {
        expect(defaultPrompt(stage), stage.id).toMatch(/^Task: \{title\}\n/)
      }
    }
  })

  it('holds valid stages with unique ids', () => {
    const ids = STAGE_CATALOG.map((entry) => entry.stage.id)
    expect(new Set(ids).size).toBe(ids.length)
    for (const entry of STAGE_CATALOG) {
      expect(stageSchema.safeParse(entry.stage).success).toBe(true)
    }
  })

  it('has a prompt for each agent stage, and a check for each output', () => {
    for (const { stage, prompt } of STAGE_CATALOG) {
      if (stage.kind === 'agent') {
        expect(prompt, stage.id).toBeDefined()
      }
      // The context stage has no prompt: its sources write the file.
      if (stage.output !== undefined && stage.kind !== 'context') {
        expect(prompt ?? '', stage.id).toContain(`{dir}/${stage.output}`)
      }
      if (prompt !== undefined && stage.output === undefined && stage.kind !== 'context') {
        // A stage that changes code must say whether to commit.
        expect(prompt, stage.id).toContain('{commit}')
      }
    }
  })

  it('has every stage of the default pipeline', () => {
    for (const step of DEFAULT_PIPELINE) {
      const stages = 'parallel' in step ? step.parallel : [step]
      for (const stage of stages) {
        expect(catalogEntry(stage.id)?.stage).toEqual(stage)
      }
    }
  })

  it('only lets agent and manual stages run side by side', () => {
    expect(GROUP_KINDS).toEqual(['agent', 'manual'])
  })
})

describe('pipelineIssues', () => {
  it('finds nothing wrong with the default pipeline', () => {
    expect(pipelineIssues(DEFAULT_PIPELINE)).toEqual([])
  })

  it('names the stage of a rule that fails', () => {
    const steps: Step[] = [
      { id: 'ship', kind: 'release', gate: 'manual' },
      { id: 'qa', kind: 'manual', gate: 'manual' },
    ]
    expect(pipelineIssues(steps)).toEqual([
      { stage: 'ship', message: 'The release stage must come last', level: 'error' },
    ])
  })

  it('names the stage that cannot have its own agent', () => {
    const steps: Step[] = [
      {
        id: 'build',
        parallel: [
          { id: 'test-spec', kind: 'agent', gate: 'manual', output: 'test-spec.md' },
          { id: 'implement', kind: 'agent', gate: 'auto', agent: 'codex' },
        ],
      },
    ]
    expect(pipelineIssues(steps)).toEqual([
      {
        stage: 'implement',
        message:
          'Only a stage outside Shell 1, or a custom stage that an agent works on, can have its own agent or arguments, and "implement" cannot',
        level: 'error',
      },
    ])
  })

  it('names the field and the stage of a wrong value', () => {
    const steps: Step[] = [{ id: 'spec', kind: 'agent', gate: 'manual', output: 'spec.txt' }]
    const [issue] = pipelineIssues(steps)
    expect(issue).toMatchObject({ stage: 'spec', level: 'error' })
    expect(issue?.message).toMatch(/^Output file: /)
  })

  it('counts a command of spaces as none, and leaves out stages that are off', () => {
    const lint: Step = { id: 'lint', kind: 'testing', gate: 'auto' }
    expect(pipelineIssues([{ ...lint, command: '   ' }]).map((issue) => issue.stage)).toEqual([
      'lint',
    ])
    const qa: Step = { id: 'qa', kind: 'manual', gate: 'manual' }
    expect(pipelineIssues([{ ...lint, enabled: false }, qa])).toEqual([])
  })

  it('needs no command on a testing stage of your own or on Testing', () => {
    expect(pipelineIssues([{ id: 'load-test', kind: 'testing', gate: 'auto' }])).toEqual([])
    expect(pipelineIssues([{ id: 'testing', kind: 'testing', gate: 'auto' }])).toEqual([])
    // A stage of your own with a catalog id but another kind needs no command either.
    expect(pipelineIssues([{ id: 'lint', kind: 'manual', gate: 'manual' }])).toEqual([])
  })

  it('names a group that is too big or holds a kind that cannot run side by side', () => {
    const agent = (id: string): Step => ({ id, kind: 'agent', gate: 'auto', output: `${id}.md` })
    const five: Step[] = [{ id: 'big', parallel: ['a', 'b', 'c', 'd', 'e'].map(agent) as never }]
    expect(pipelineIssues(five).map((issue) => issue.stage)).toEqual(['big'])
    const testing: Step[] = [
      {
        id: 'mixed',
        parallel: [agent('a'), { id: 't', kind: 'testing', gate: 'auto' }] as never,
      },
    ]
    expect(pipelineIssues(testing).length).toBeGreaterThan(0)
    expect(pipelineIssues(testing).every((issue) => issue.level === 'error')).toBe(true)
  })

  it('counts two problems as two', () => {
    const steps: Step[] = [
      { id: 'ship', kind: 'release', gate: 'manual' },
      { id: 'qa', kind: 'manual', gate: 'manual' },
      { id: 'context', kind: 'context', gate: 'manual' },
    ]
    expect(pipelineIssues(steps).map((issue) => issue.message)).toEqual([
      'The context stage must come first',
      'The release stage must come last',
    ])
  })

  it('names the second stage of an id used twice', () => {
    const steps: Step[] = [
      { id: 'qa', kind: 'manual', gate: 'manual' },
      { id: 'qa', kind: 'manual', gate: 'manual' },
    ]
    expect(pipelineIssues(steps)).toEqual([
      { stage: 'qa', message: 'The stage id "qa" is used twice', level: 'error' },
    ])
  })

  it('asks for a command where the catalog needs one', () => {
    const lint: Step = { id: 'lint', kind: 'testing', gate: 'auto' }
    expect(pipelineIssues([lint])).toEqual([
      { stage: 'lint', message: 'Give the command to run', level: 'error' },
    ])
    expect(pipelineIssues([{ ...lint, command: 'pnpm lint' }])).toEqual([])
  })

  it('warns about two code-changing stages side by side, without stopping a save', () => {
    const steps: Step[] = [
      {
        id: 'build',
        parallel: [
          { id: 'implement', kind: 'agent', gate: 'auto' },
          { id: 'docs', kind: 'agent', gate: 'auto' },
        ],
      },
    ]
    expect(pipelineIssues(steps)).toEqual([
      {
        stage: 'build',
        message: '2 stages without an output file change code in one worktree at the same time',
        level: 'warning',
      },
    ])
  })
})
