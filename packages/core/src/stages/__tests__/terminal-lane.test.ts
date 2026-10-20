import { describe, expect, it } from 'vitest'
import type { TaskState } from '../../tasks/task-state.js'
import { DEFAULT_PIPELINE } from '../default-pipeline.js'
import { pipelineSchema } from '../pipeline-schema.js'
import { combinedState } from '../position.js'
import { type BranchPath, type Stage, type Step, stagesOf } from '../schema.js'
import {
  endsShell1,
  stageAgent,
  stageOfTerminal,
  terminalAtStart,
  terminalLane,
  terminalOfStage,
} from '../terminal-lane.js'

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

  it('gets Shell 6 plus its index among all stages, and Shell 1 keeps the rest', () => {
    const shells = steps.map((step) => terminalOfStage(steps, step.id))
    expect(shells).toEqual(['1', '7', '1', '9', '10', '11'])
    expect(stageOfTerminal(steps, steps[4] as Step, '10')?.id).toBe('ci-watch')
    expect(stageOfTerminal(steps, steps[4] as Step, '1')).toBeUndefined()
    const position = { stage: 'ci-watch', state: 'running' as const, lanes: {} }
    expect(terminalLane(position, steps, '10')).toEqual({ stage: 'ci-watch', state: 'running' })
    // Shell 1 served Spec and Implement; the last of them names it.
    expect(terminalLane(position, steps, '1')).toEqual({ stage: 'implement', state: 'done' })
  })

  it('keeps the stage of a shell after its step is over', () => {
    const atImplement = { stage: 'implement', state: 'running' as const, lanes: {} }
    expect(terminalLane(atImplement, steps, '7')).toEqual({ stage: 'test-spec', state: 'done' })
    // A stage that has not run yet does not name a shell.
    expect(terminalLane(atImplement, steps, '9')).toEqual({ stage: null, state: null })
    expect(terminalLane(atImplement, steps, '2')).toEqual({ stage: null, state: null })
    const atSpec = { stage: 'spec', state: 'running' as const, lanes: {} }
    expect(terminalLane(atSpec, steps, '7')).toEqual({ stage: null, state: null })
    const finished = { stage: 'merge-pr', state: 'done' as const, lanes: {} }
    expect(terminalLane(finished, steps, '7')).toEqual({ stage: 'test-spec', state: 'done' })
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
    expect(ids.map((id) => terminalOfStage(group, id))).toEqual(['1', '7', '3', '9'])
    expect(stageOfTerminal(group, group[0] as Step, '9')?.id).toBe('lint-notes')
  })

  it('never gives two stages the same shell, whatever the settings say', () => {
    const all: Step[] = [
      own('spec', { output: 'spec.md', ownShell: true }),
      own('test-spec', { output: 'test-spec.md', ownShell: true }),
      { id: 'build', parallel: [own('implement'), own('notes', { output: 'notes.md' })] },
    ]
    const shells = all.flatMap(stagesOf).map((stage) => terminalOfStage(all, stage.id))
    expect(shells).toEqual(['6', '7', '1', '9'])
    // A stage turned off in the settings still counts, so the numbers after it stay.
    const off: Step[] = [{ ...(all[0] as Stage), enabled: false }, ...all.slice(1)]
    expect(terminalOfStage(off, 'test-spec')).toBe('7')
  })

  it('gives no shell to an agent the stage may not have', () => {
    const invalid: Step[] = [{ id: 'spec', kind: 'agent', gate: 'manual', agent: 'codex' }]
    expect(terminalOfStage(invalid, 'spec')).toBe('1')
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
      `Only an agent stage with an output file can run in its own terminal, and "${id}" cannot`
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

describe('a branch', () => {
  const plan: Stage = { id: 'plan', kind: 'agent', gate: 'manual', output: 'plan.md' }
  const implement: Stage = { id: 'implement', kind: 'agent', gate: 'auto' }
  const testing: Stage = { id: 'testing', kind: 'testing', gate: 'auto' }
  const small: BranchPath = {
    id: 'small',
    name: 'Small change',
    when: { type: 'output', file: 'plan.md', section: 'Size', contains: 'small' },
    stages: [implement, testing],
  }

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
    expect(ids.map((id) => terminalOfStage(shells, id))).toEqual(['1', '1', '5', '9', '10'])
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
    expect(terminalLane(atNotes, shells, '10')).toEqual({ stage: 'notes', state: 'running' })
    // Implement is on the path the task did not take.
    expect(terminalLane(atNotes, shells, '1')).toEqual({ stage: 'plan', state: 'done' })
    expect(terminalLane(atNotes, shells, '9')).toEqual({ stage: 'write-docs', state: 'done' })
  })
})

describe('a terminal pinned at the start of its stage', () => {
  // T-0136: Test spec runs in its own shell. Then the settings give Spec its own agent.
  const testSpec: Stage = {
    id: 'test-spec',
    kind: 'agent',
    gate: 'manual',
    output: 'test-spec.md',
    ownShell: true,
    agent: 'copilot',
  }
  const before: Step[] = [
    { id: 'spec', kind: 'agent', gate: 'manual', output: 'spec.md' },
    testSpec,
    { id: 'implement', kind: 'agent', gate: 'auto' },
  ]
  const after: Step[] = [
    {
      id: 'spec',
      kind: 'agent',
      gate: 'manual',
      output: 'spec.md',
      ownShell: true,
      agent: 'codex',
    },
    ...before.slice(1),
  ]
  const lost: Step[] = [before[0] as Step, { ...testSpec, agent: undefined }, before[2] as Step]
  const pinned = { spec: '1', 'test-spec': terminalOfStage(before, 'test-spec') }

  it('keeps the stage of a running agent when the settings change', () => {
    expect(terminalOfStage(after, 'test-spec', pinned)).toBe(pinned['test-spec'])
    expect(stageOfTerminal(after, testSpec, pinned['test-spec'], pinned)?.id).toBe('test-spec')
    const position = { stage: 'test-spec', state: 'running' as const, lanes: {} }
    expect(terminalLane(position, after, pinned['test-spec'], pinned)).toEqual({
      stage: 'test-spec',
      state: 'running',
    })
    // Spec ran in Shell 1, so Shell 1 still names it.
    expect(terminalLane(position, after, '1', pinned)).toEqual({ stage: 'spec', state: 'done' })
  })

  it('keeps it when the stage loses its own agent while it runs', () => {
    expect(terminalOfStage(lost, 'test-spec')).toBe('5')
    expect(terminalOfStage(lost, 'test-spec', pinned)).toBe('7')
  })

  it('keeps it at the next start while the stage needs the same kind of shell', () => {
    expect(terminalAtStart(after, 'test-spec', pinned)).toBe('7')
    expect(terminalAtStart(after, 'implement', pinned)).toBe('1')
  })

  it('takes the shell of the settings at the next start when the stage gained or lost its agent', () => {
    // Spec gained its own agent: it gets its own shell, never one another agent holds.
    expect(terminalAtStart(after, 'spec', pinned)).toBe('6')
    expect(terminalAtStart(lost, 'test-spec', pinned)).toBe('5')
  })
})
