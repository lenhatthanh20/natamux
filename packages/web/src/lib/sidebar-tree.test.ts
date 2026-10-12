import { type Task, taskSchema } from '@natamux/core'
import { describe, expect, it } from 'vitest'
import type { TaskShells } from './api'
import {
  cometHead,
  cometOf,
  isFoldOpen,
  opensByDefault,
  parseFolds,
  type ShellRow,
  shellRowName,
  shellRows,
  shellTag,
  stagePosition,
  subtaskCount,
  taskGlyphState,
  urgentState,
  withFold,
  withParents,
} from './sidebar-tree'

function makeTask(overrides: Partial<Task>): Task {
  return taskSchema.parse({
    id: 'T-0001',
    title: 'Example',
    state: 'running',
    workspace: '/work/T-0001',
    createdAt: '2026-10-08T09:00:00.000Z',
    updatedAt: '2026-10-08T09:00:00.000Z',
    ...overrides,
  })
}

describe('folds', () => {
  it('reads saved folds and drops anything that is not true or false', () => {
    expect(parseFolds('{"repo:a":false,"task:T-1":true,"bad":"yes"}')).toEqual({
      'repo:a': false,
      'task:T-1': true,
    })
    expect(parseFolds('[1]')).toEqual({})
    expect(parseFolds('not json')).toEqual({})
    expect(parseFolds(null)).toEqual({})
  })

  it('uses the default until the user picks', () => {
    expect(isFoldOpen({}, 'task:T-1', true)).toBe(true)
    expect(isFoldOpen({ 'task:T-1': false }, 'task:T-1', true)).toBe(false)
  })

  it('keeps only the newest choices', () => {
    let folds: Record<string, boolean> = {}
    for (let index = 0; index < 305; index += 1) {
      folds = withFold(folds, `task:T-${index}`, true)
    }
    expect(Object.keys(folds)).toHaveLength(300)
    expect(folds['task:T-0']).toBeUndefined()
    expect(folds['task:T-304']).toBe(true)
  })
})

describe('urgentState', () => {
  it('picks the state that needs the user most', () => {
    expect(urgentState(['running', 'failed', 'needs-input'])).toBe('needs-input')
    expect(urgentState(['ready', 'running', 'failed'])).toBe('failed')
    expect(urgentState(['ready', 'review', 'blocked'])).toBe('blocked')
    expect(urgentState(['ready', 'review'])).toBe('review')
    expect(urgentState(['ready', 'running'])).toBe('running')
    expect(urgentState(['done', 'ready'])).toBe('ready')
  })

  it('is null when nothing is open', () => {
    expect(urgentState(['done', 'cancelled', 'draft'])).toBeNull()
    expect(urgentState([])).toBeNull()
  })
})

describe('opensByDefault', () => {
  it('opens when a child works, waits or failed', () => {
    expect(opensByDefault(['ready', 'running'])).toBe(true)
    expect(opensByDefault(['needs-input'])).toBe(true)
    expect(opensByDefault(['failed'])).toBe(true)
    expect(opensByDefault(['ready', 'review', 'done'])).toBe(false)
  })
})

describe('stagePosition', () => {
  it('counts the current step from the stages of the task', () => {
    const task = makeTask({ stage: 'review', stages: ['spec', 'implement', 'review', 'release'] })
    expect(stagePosition(task)).toEqual({ at: 3, of: 4 })
  })

  it('counts a group of parallel stages as one step, by its id or one of its stages', () => {
    const stages = ['spec', { id: 'checks', parallel: ['lint', 'security'] }, 'release']
    expect(stagePosition(makeTask({ stage: 'checks', stages }))).toEqual({ at: 2, of: 3 })
    expect(stagePosition(makeTask({ stage: 'security', stages }))).toEqual({ at: 2, of: 3 })
  })

  it('is null for one step, an unknown stage, or a task with no stage to show', () => {
    expect(stagePosition(makeTask({ stage: 'implement', stages: ['implement'] }))).toBeNull()
    expect(stagePosition(makeTask({ stage: 'other', stages: ['spec', 'implement'] }))).toBeNull()
    const stages = ['spec', 'implement']
    expect(stagePosition(makeTask({ state: 'done', stage: 'implement', stages }))).toBeNull()
    expect(stagePosition(makeTask({ state: 'draft', stage: 'spec', stages }))).toBeNull()
  })
})

describe('subtaskCount', () => {
  it('counts every subtask of the task, and the done ones', () => {
    const tasks = [
      makeTask({ id: 'T-0001' }),
      makeTask({ id: 'T-0002', parentId: 'T-0001', state: 'done' }),
      makeTask({ id: 'T-0003', parentId: 'T-0001', workspace: null, state: 'ready' }),
      makeTask({ id: 'T-0004', parentId: 'T-0009' }),
    ]
    expect(subtaskCount('T-0001', tasks)).toEqual({ done: 1, total: 2 })
    expect(subtaskCount('T-0004', tasks)).toBeNull()
  })
})

describe('withParents', () => {
  it('brings back the parent of a subtask that the filter matched', () => {
    const parent = makeTask({ id: 'T-0001', title: 'Parent' })
    const child = makeTask({ id: 'T-0002', title: 'Child', parentId: 'T-0001' })
    const other = makeTask({ id: 'T-0003', title: 'Other' })
    const all = [parent, child, other]
    expect(withParents([child], all).map((task) => task.id)).toEqual(['T-0002', 'T-0001'])
    expect(withParents([parent, child], all).map((task) => task.id)).toEqual(['T-0001', 'T-0002'])
    expect(withParents([other], all)).toEqual([other])
  })

  it('does not bring back a parent without a workspace', () => {
    const parent = makeTask({ id: 'T-0001', workspace: null })
    const child = makeTask({ id: 'T-0002', parentId: 'T-0001' })
    expect(withParents([child], [parent, child])).toEqual([child])
  })
})

const shells = (
  lanes: TaskShells['lanes'],
  programs: TaskShells['programs'] = {},
  terminals = [...new Set([...Object.keys(lanes), ...Object.keys(programs)])],
): TaskShells => ({ terminals, lanes, programs })

describe('shellRows', () => {
  const task = makeTask({
    stage: 'checks',
    state: 'needs-input',
    lanes: { alpha: 'running', beta: 'needs-input' },
  })

  it('stands in a row per parallel stage before the shells are known', () => {
    expect(shellRows(task)).toEqual([
      {
        key: 'lane:alpha',
        label: 'Alpha',
        agent: null,
        state: 'running',
        shell: null,
        plain: false,
      },
      {
        key: 'lane:beta',
        label: 'Beta',
        agent: null,
        state: 'needs-input',
        shell: null,
        plain: false,
      },
    ])
  })

  it('gives each shell a row in shell order, with its stage, agent and state', () => {
    const rows = shellRows(
      task,
      shells(
        {
          '1': { stage: 'alpha', state: null, agent: 'claude-code' },
          '2': { stage: 'beta', state: null, agent: 'codex' },
        },
        { '10': 'zsh' },
        ['10', '2', '1'],
      ),
    )
    expect(rows.map((row) => [row.key, row.label, row.agent, row.state, row.plain])).toEqual([
      ['shell:1', 'Alpha', 'claude-code', 'running', false],
      ['shell:2', 'Beta', 'codex', 'needs-input', false],
      ['shell:10', 'zsh', null, null, true],
    ])
  })

  it('names an agent on no stage after its shell, with the state its hooks gave', () => {
    const rows = shellRows(
      makeTask({ stage: 'spec', state: 'running' }),
      shells({
        '1': { stage: 'spec', state: 'running', agent: 'claude-code' },
        '2': { stage: null, state: null, agent: 'cursor' },
        '3': { stage: null, state: 'needs-input', agent: 'cursor' },
      }),
    )
    expect(rows.map((row) => [row.label, row.agent, row.state])).toEqual([
      ['Spec', 'claude-code', 'running'],
      ['Shell 2', 'cursor', null],
      ['Shell 3', 'cursor', 'needs-input'],
    ])
  })

  it('names a plain shell after its program, else "Shell"', () => {
    const rows = shellRows(makeTask({}), shells({}, { '1': 'bash', '2': null }))
    expect(rows.map((row) => [row.label, row.plain, row.shell])).toEqual([
      ['bash', true, '1'],
      ['Shell', true, '2'],
    ])
  })

  it('keeps a parallel stage that no shell works on', () => {
    const rows = shellRows(task, shells({ '1': { stage: 'alpha', state: null } }))
    expect(rows.map((row) => row.key)).toEqual(['shell:1', 'lane:beta'])
  })
})

describe('shellRowName', () => {
  const row = (fields: Partial<ShellRow>): ShellRow => ({
    key: 'k',
    label: 'Implement',
    agent: 'claude-code',
    state: 'running',
    shell: '1',
    plain: false,
    ...fields,
  })

  it('says the stage, the agent, the shell and the state', () => {
    expect(shellRowName(row({}))).toBe('Implement, Claude Code, Shell 1, Running')
    const byHand = row({ label: 'Shell 2', shell: '2', agent: 'cursor', state: null })
    expect(shellRowName(byHand)).toBe('Shell 2, Cursor')
    const plain = row({ label: 'zsh', shell: '2', agent: null, state: null, plain: true })
    expect(shellRowName(plain)).toBe('Shell 2, zsh')
    expect(shellRowName(row({ label: 'Beta', shell: null, agent: null }))).toBe('Beta, Running')
  })

  it('tags a shell only where the name does not say it', () => {
    expect(shellTag(row({}))).toBe('S1')
    expect(shellTag(row({ label: 'Shell 2', shell: '2' }))).toBeNull()
    expect(shellTag(row({ label: 'zsh', shell: '2', plain: true }))).toBe('S2')
    expect(shellTag(row({ shell: null }))).toBeNull()
  })
})

describe('taskGlyphState', () => {
  const rows = (...states: ShellRow['state'][]): ShellRow[] =>
    states.map((state, index) => ({
      key: `shell:${index + 1}`,
      label: 'x',
      agent: 'claude-code',
      state,
      shell: String(index + 1),
      plain: false,
    }))

  it('spins while any row runs, and else shows the task state', () => {
    const waiting = makeTask({ state: 'needs-input' })
    expect(taskGlyphState(makeTask({ state: 'running' }), rows('running'))).toBe('running')
    expect(taskGlyphState(waiting, rows('needs-input', 'running'))).toBe('running')
    expect(taskGlyphState(waiting, rows('needs-input', 'done'))).toBe('needs-input')
    expect(taskGlyphState(makeTask({ state: 'review' }), [])).toBe('review')
  })

  it('spins for a running parallel stage while the shells are not read', () => {
    const task = makeTask({ state: 'needs-input', lanes: { a: 'needs-input', b: 'running' } })
    expect(taskGlyphState(task, [])).toBe('running')
  })
})

describe('cometOf', () => {
  const steps = ['context', 'spec', 'plan', { id: 'build', parallel: ['test-spec', 'implement'] }]
  const six = [...steps, 'testing', 'review']

  it('puts the head at the current step, counting it', () => {
    const comet = cometOf(makeTask({ stages: six, stage: 'plan', state: 'running' }))
    expect(comet.progress).toBe(0.5)
    expect(comet.at).toBe(3)
    expect(comet.of).toBe(6)
    expect(comet.state).toBe('running')
    expect(comet.stage).toBe('Plan')
    const first = cometOf(makeTask({ stages: six, stage: 'context', state: 'running' }))
    expect(first.progress).toBe(1 / 6)
  })

  it('names the most urgent stage of a group, and takes its state', () => {
    const waiting = cometOf(
      makeTask({
        stages: six,
        stage: 'build',
        state: 'needs-input',
        lanes: { 'test-spec': 'done', implement: 'needs-input' },
      }),
    )
    expect(waiting).toMatchObject({ stage: 'Implement', state: 'needs-input', at: 4 })
    const failed = cometOf(
      makeTask({
        stages: six,
        stage: 'build',
        state: 'failed',
        lanes: { 'test-spec': 'failed', implement: 'running' },
      }),
    )
    expect(failed).toMatchObject({ stage: 'Test spec', state: 'failed' })
  })

  it('fills a finished task, and a pipeline of one step', () => {
    expect(cometOf(makeTask({ stages: six, stage: 'review', state: 'done' }))).toMatchObject({
      progress: 1,
      state: 'done',
      stage: null,
    })
    expect(cometOf(makeTask({ stages: ['implement'], stage: 'implement' })).progress).toBe(1)
  })

  it('lists every step with its state for the tooltip', () => {
    const comet = cometOf(
      makeTask({
        stages: six,
        stage: 'build',
        state: 'needs-input',
        lanes: { 'test-spec': 'done', implement: 'needs-input' },
      }),
    )
    expect(comet.summary).toBe(
      'Context done, Spec done, Plan done, Test spec done, Implement needs your input, Automated tests not started, Manual review not started',
    )
  })
})

describe('cometHead', () => {
  it('keeps the head inside the track', () => {
    expect(cometHead(0.5)).toBe(12)
    expect(cometHead(0)).toBe(2.4)
    expect(cometHead(1)).toBe(21.6)
  })
})
