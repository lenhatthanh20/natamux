import { describe, expect, it } from 'vitest'
import { pipelineIssues } from './pipeline-issues.js'
import { DEFAULT_PIPELINE, GROUP_KINDS, type Step, stageSchema } from './stage.js'
import { catalogEntry, STAGE_CATALOG } from './stage-catalog.js'

describe('STAGE_CATALOG', () => {
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

  it('names the field and the stage of a wrong value', () => {
    const steps: Step[] = [{ id: 'spec', kind: 'agent', gate: 'manual', output: 'spec.txt' }]
    const [issue] = pipelineIssues(steps)
    expect(issue).toMatchObject({ stage: 'spec', level: 'error' })
    expect(issue?.message).toMatch(/^Output file: /)
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
