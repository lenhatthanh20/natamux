import { applyTaskEvent, LEGACY_PIPELINE, TASK_STATES, type TaskState } from '@natamux/core'
import { describe, expect, it } from 'vitest'
import { AGENT_SIGNALS, type AgentSignal, planSignalEvents, toolSummary } from '../agent-signals.js'

function types(state: TaskState, signal: AgentSignal): string[] {
  return planSignalEvents(state, signal, {}).map((planned) => planned.type)
}

describe('planSignalEvents', () => {
  it.each<[TaskState, string[]]>([
    ['draft', ['task.readied', 'agent.started']],
    ['ready', ['agent.started']],
    ['needs-input', ['agent.resumed']],
    ['review', ['agent.resumed']],
    ['running', []],
    ['failed', ['agent.resumed']],
    ['blocked', []],
    ['done', []],
    ['cancelled', []],
  ])('maps a prompt in a task that is %s', (state, expected) => {
    expect(types(state, 'prompt')).toEqual(expected)
  })

  it('treats the end of a turn as finished when the agent was running', () => {
    expect(types('running', 'turn-ended')).toEqual(['agent.finished'])
  })

  it('first resumes, then finishes, when the agent was waiting for the user', () => {
    expect(types('needs-input', 'turn-ended')).toEqual(['agent.resumed', 'agent.finished'])
  })

  it.each<TaskState>(['draft', 'ready', 'review', 'blocked', 'failed', 'done', 'cancelled'])(
    'ignores the end of a turn in a task that is %s',
    (state) => {
      expect(types(state, 'turn-ended')).toEqual([])
    },
  )

  it('keeps a running task running when the agent pauses for background work', () => {
    expect(types('running', 'turn-paused')).toEqual(['agent.waiting'])
  })

  it('first resumes, then waits, when the agent was waiting for the user', () => {
    expect(types('needs-input', 'turn-paused')).toEqual(['agent.resumed', 'agent.waiting'])
  })

  it.each<TaskState>(['draft', 'ready', 'review', 'blocked', 'failed', 'done', 'cancelled'])(
    'ignores a paused turn in a task that is %s',
    (state) => {
      expect(types(state, 'turn-paused')).toEqual([])
    },
  )

  it('maps a failure to failed', () => {
    expect(types('running', 'failed')).toEqual(['agent.failed'])
    expect(types('needs-input', 'failed')).toEqual(['agent.resumed', 'agent.failed'])
  })

  it('does not fail a failed task twice', () => {
    expect(types('failed', 'failed')).toEqual([])
  })

  it('maps a question of a running agent to needs input, and ignores it otherwise', () => {
    expect(types('running', 'asking')).toEqual(['agent.asked'])
    expect(types('ready', 'asking')).toEqual([])
    expect(types('review', 'asking')).toEqual([])
  })

  it('resumes a waiting task when the agent works, and ignores the call otherwise', () => {
    expect(types('needs-input', 'activity')).toEqual(['agent.resumed'])
    expect(types('running', 'activity')).toEqual([])
  })

  // After an API error the session lives on, so a tool that runs shows the agent works again.
  it('resumes a failed task when the agent works again', () => {
    expect(types('failed', 'activity')).toEqual(['agent.resumed'])
  })

  it('passes the payload to every event it plans', () => {
    const planned = planSignalEvents('draft', 'prompt', { hook: 'x' })
    expect(planned.map((event) => event.payload)).toEqual([{ hook: 'x' }, { hook: 'x' }])
  })

  it('only plans events the state machine accepts, in every state', () => {
    for (const start of TASK_STATES) {
      for (const signal of AGENT_SIGNALS) {
        let position = {
          stage: start === 'review' ? 'review' : 'implement',
          state: start,
          lanes: {},
        }
        for (const planned of planSignalEvents(start, signal, {})) {
          const applied = applyTaskEvent(
            position,
            {
              taskId: 'T-0001',
              source: 'agent',
              type: planned.type,
              at: '2026-10-08T09:00:00.000Z',
              payload: {},
            },
            LEGACY_PIPELINE,
          )
          expect(applied.accepted, `${signal} in ${start}`).toBe(true)
          position = applied.position
        }
      }
    }
  })
})

describe('toolSummary', () => {
  const summary = (tool: string, toolInput: Record<string, unknown>) =>
    toolSummary({
      hook_event_name: 'PreToolUse',
      cwd: '/w',
      tool_name: tool,
      tool_input: toolInput,
    })

  it.each([
    ['Edit', { file_path: '/w/src/a.ts' }, 'Edit src/a.ts'],
    ['Read', { file_path: '/other/b.ts' }, 'Read /other/b.ts'],
    ['Bash', { command: 'pnpm  test\n--run' }, 'Bash: pnpm test --run'],
    ['Grep', { pattern: 'TODO' }, 'Grep TODO'],
    ['TodoWrite', {}, 'TodoWrite'],
  ])('says %s %j as %j', (tool, toolInput, text) => {
    expect(summary(tool, toolInput)).toBe(text)
  })

  it('cuts a long line', () => {
    expect(summary('Bash', { command: 'x'.repeat(200) })).toHaveLength(80)
  })

  it('has nothing to say without a tool', () => {
    expect(toolSummary({ hook_event_name: 'Stop' })).toBeUndefined()
  })
})
