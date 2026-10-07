import { mkdirSync, mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { applyTaskEvent, TASK_STATES, type TaskState } from '@helmsman/core'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import {
  ClaudeCodeHookService,
  type HookInput,
  hookInputSchema,
  planHookEvents,
} from './claude-code-hooks.js'
import { EventBus } from './event-bus.js'
import { TaskIndex } from './task-index.js'
import { TaskService } from './task-service.js'

function hook(name: string, extra: Partial<HookInput> = {}): HookInput {
  return { hook_event_name: name, ...extra }
}

function types(state: TaskState, input: HookInput): string[] {
  return planHookEvents(state, input).map((planned) => planned.type)
}

describe('planHookEvents', () => {
  it.each<[TaskState, string[]]>([
    ['draft', ['task.readied', 'agent.started']],
    ['ready', ['agent.started']],
    ['needs-input', ['agent.resumed']],
    ['review', ['agent.resumed']],
    ['running', []],
    ['blocked', []],
    ['failed', []],
    ['done', []],
    ['cancelled', []],
  ])('maps a prompt in a task that is %s', (state, expected) => {
    expect(types(state, hook('UserPromptSubmit'))).toEqual(expected)
  })

  it('treats the end of a turn as finished when the agent was running', () => {
    expect(types('running', hook('Stop'))).toEqual(['agent.finished'])
  })

  it('first resumes, then finishes, when the agent was waiting for the user', () => {
    expect(types('needs-input', hook('Stop'))).toEqual(['agent.resumed', 'agent.finished'])
  })

  it.each<TaskState>(['draft', 'ready', 'review', 'blocked', 'failed', 'done', 'cancelled'])(
    'ignores the end of a turn in a task that is %s',
    (state) => {
      expect(types(state, hook('Stop'))).toEqual([])
    },
  )

  it('maps an API error to failed', () => {
    expect(types('running', hook('StopFailure'))).toEqual(['agent.failed'])
    expect(types('needs-input', hook('StopFailure'))).toEqual(['agent.resumed', 'agent.failed'])
  })

  it('maps a permission request of a running agent to needs input', () => {
    expect(types('running', hook('PermissionRequest'))).toEqual(['agent.asked'])
    expect(types('ready', hook('PermissionRequest'))).toEqual([])
  })

  it.each([
    ['permission_prompt', ['agent.asked']],
    ['elicitation_dialog', ['agent.asked']],
    ['agent_needs_input', ['agent.asked']],
    ['idle_prompt', []],
    ['auth_success', []],
  ])('maps the notification %s', (notificationType, expected) => {
    expect(types('running', hook('Notification', { notification_type: notificationType }))).toEqual(
      expected,
    )
  })

  it('ignores a notification without a type', () => {
    expect(types('running', hook('Notification'))).toEqual([])
  })

  it('resumes a waiting task when a tool runs, and ignores the call otherwise', () => {
    expect(types('needs-input', hook('PostToolUse'))).toEqual(['agent.resumed'])
    expect(types('running', hook('PostToolUse'))).toEqual([])
  })

  it('ignores hooks it does not know', () => {
    expect(types('running', hook('SessionEnd'))).toEqual([])
  })

  it('keeps the details of the hook in the payload and cuts a long message', () => {
    const [planned] = planHookEvents(
      'running',
      hook('Notification', {
        notification_type: 'permission_prompt',
        session_id: 's1',
        message: 'x'.repeat(900),
      }),
    )
    expect(planned?.payload).toMatchObject({
      hook: 'Notification',
      notificationType: 'permission_prompt',
      sessionId: 's1',
    })
    expect(String(planned?.payload.message)).toHaveLength(500)
  })

  it('only plans events the state machine accepts, in every state', () => {
    const inputs = [
      hook('UserPromptSubmit'),
      hook('PostToolUse'),
      hook('PermissionRequest'),
      hook('Notification', { notification_type: 'permission_prompt' }),
      hook('Stop'),
      hook('StopFailure'),
    ]
    for (const start of TASK_STATES) {
      for (const input of inputs) {
        let state = start
        for (const planned of planHookEvents(start, input)) {
          const applied = applyTaskEvent(state, {
            taskId: 'T-0001',
            source: 'agent',
            type: planned.type,
            at: '2026-10-08T09:00:00.000Z',
            payload: {},
          })
          expect(applied.accepted, `${input.hook_event_name} in ${start}`).toBe(true)
          state = applied.state
        }
      }
    }
  })
})

describe('hookInputSchema', () => {
  it('needs an event name and drops the fields it does not read', () => {
    expect(hookInputSchema.safeParse({}).success).toBe(false)
    expect(
      hookInputSchema.parse({ hook_event_name: 'Stop', tool_input: { command: 'ls' } }),
    ).toEqual({ hook_event_name: 'Stop' })
  })
})

describe('ClaudeCodeHookService', () => {
  let root: string
  let index: TaskIndex
  let tasks: TaskService
  let service: ClaudeCodeHookService

  beforeEach(() => {
    root = mkdtempSync(join(tmpdir(), 'helmsman-hooks-'))
    const dataDir = join(root, 'data')
    mkdirSync(dataDir)
    index = new TaskIndex(':memory:')
    tasks = new TaskService({ dataDir, index, bus: new EventBus() })
    service = new ClaudeCodeHookService(tasks)
  })

  afterEach(() => {
    index.close()
    rmSync(root, { recursive: true, force: true })
  })

  it('finds the task by the header', () => {
    const task = tasks.createTask({ title: 'Fix it' })
    expect(service.findTask('/anywhere', task.id)?.id).toBe(task.id)
  })

  it('finds the task by the workspace that holds the working directory', () => {
    const task = tasks.createTask({ title: 'Fix it' })
    tasks.recordWorkspace(task.id, { workspace: '/work/T-0001', branch: 'b' })

    expect(service.findTask('/work/T-0001', undefined)?.id).toBe(task.id)
    expect(service.findTask('/work/T-0001/packages/web', undefined)?.id).toBe(task.id)
  })

  it('does not mix up workspaces whose paths start the same way', () => {
    const task = tasks.createTask({ title: 'Fix it' })
    tasks.recordWorkspace(task.id, { workspace: '/work/T-0001', branch: 'b' })

    expect(service.findTask('/work/T-00010', undefined)).toBeUndefined()
  })

  it('falls back to the working directory when the header is empty, invalid or unknown', () => {
    const task = tasks.createTask({ title: 'Fix it' })
    tasks.recordWorkspace(task.id, { workspace: '/work/T-0001', branch: 'b' })

    for (const header of [undefined, 'nope', 'T-0099']) {
      expect(service.findTask('/work/T-0001', header)?.id).toBe(task.id)
    }
  })

  it('finds nothing for a directory that belongs to no task', () => {
    tasks.createTask({ title: 'Fix it' })
    expect(service.findTask('/elsewhere', undefined)).toBeUndefined()
    expect(service.findTask(undefined, undefined)).toBeUndefined()
  })

  it('moves a task through a run: prompt, question, answer, end of turn', () => {
    const task = tasks.createTask({ title: 'Fix it' })
    const step = (name: string, extra: Partial<HookInput> = {}) =>
      service.handle(hook(name, extra), task.id)

    expect(step('UserPromptSubmit').applied).toEqual(['task.readied', 'agent.started'])
    expect(tasks.getTask(task.id).state).toBe('running')

    step('PermissionRequest')
    expect(tasks.getTask(task.id).state).toBe('needs-input')

    step('PostToolUse')
    expect(tasks.getTask(task.id).state).toBe('running')

    step('Stop')
    expect(tasks.getTask(task.id).state).toBe('review')
  })

  it('records the events as coming from the agent', () => {
    const task = tasks.createTask({ title: 'Fix it' })
    service.handle(hook('UserPromptSubmit'), task.id)

    const sources = tasks.listEvents(task.id).map((entry) => entry.event.source)
    expect(sources).toEqual(['system', 'agent', 'agent'])
  })

  it('does nothing, and logs nothing, when the hook changes nothing', () => {
    const task = tasks.createTask({ title: 'Fix it' })
    const result = service.handle(hook('Stop'), task.id)

    expect(result).toEqual({ taskId: task.id, applied: [] })
    expect(tasks.listEvents(task.id)).toHaveLength(1)
  })

  it('reports that no task was found', () => {
    expect(service.handle(hook('Stop', { cwd: '/elsewhere' }), undefined)).toEqual({
      taskId: null,
      applied: [],
    })
  })
})
