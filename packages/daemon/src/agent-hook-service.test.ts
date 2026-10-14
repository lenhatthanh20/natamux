import { mkdirSync, mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { findAdapter, knownAgentIds } from './agent-adapters.js'
import { AgentHookService, UnknownAgentError } from './agent-hook-service.js'
import { type DaemonMessage, EventBus } from './event-bus.js'
import { TaskIndex } from './task-index.js'
import { TaskService } from './task-service.js'

describe('the adapter list', () => {
  it('knows Claude Code', () => {
    expect(knownAgentIds()).toContain('claude-code')
    expect(findAdapter('claude-code')?.id).toBe('claude-code')
  })

  it('knows no adapter for another name', () => {
    expect(findAdapter('nope')).toBeUndefined()
  })
})

describe('AgentHookService', () => {
  let root: string
  let index: TaskIndex
  let tasks: TaskService
  let service: AgentHookService

  const hook = (name: string, extra: Record<string, unknown> = {}) => ({
    hook_event_name: name,
    ...extra,
  })

  beforeEach(() => {
    root = mkdtempSync(join(tmpdir(), 'natamux-hooks-'))
    const dataDir = join(root, 'data')
    mkdirSync(dataDir)
    index = new TaskIndex(':memory:')
    tasks = new TaskService({ dataDir, index, bus: new EventBus() })
    service = new AgentHookService(tasks)
  })

  afterEach(() => {
    index.close()
    rmSync(root, { recursive: true, force: true })
  })

  it('keeps what the agent does now, and forgets it when the turn ends', () => {
    const task = tasks.createTask({ title: 'Fix it' })
    service.handle('claude-code', hook('UserPromptSubmit'), task.id)
    expect(service.activity()[task.id]?.['1']?.text).toBe('Reading the prompt')

    service.handle(
      'claude-code',
      hook('PreToolUse', { tool_name: 'Bash', tool_input: { command: 'pnpm test' } }),
      task.id,
    )
    expect(service.activity()[task.id]?.['1']?.text).toBe('Bash: pnpm test')

    service.handle('claude-code', hook('Stop'), task.id)
    expect(service.activity()).toEqual({})
  })

  it('keeps what the agent of each terminal does apart', () => {
    const task = tasks.createTask({ title: 'Fix it' })
    // Hooks from a terminal count only while the task has its workspace.
    tasks.recordWorkspace(task.id, { workspace: '/work/T-0001', branch: 'b' })
    const edit = (path: string) =>
      hook('PreToolUse', { tool_name: 'Edit', tool_input: { file_path: path } })

    service.handle('claude-code', edit('src/a.ts'), task.id, '1')
    service.handle('claude-code', edit('test-spec.md'), task.id, '2')
    expect(service.activity()[task.id]?.['1']?.text).toBe('Edit src/a.ts')
    expect(service.activity()[task.id]?.['2']?.text).toBe('Edit test-spec.md')

    // The turn end in Shell 2 clears only its own line.
    service.handle('claude-code', hook('Stop'), task.id, '2')
    expect(Object.keys(service.activity()[task.id] ?? {})).toEqual(['1'])
    expect(service.activity()[task.id]?.['1']?.text).toBe('Edit src/a.ts')

    // A hook without a terminal comes from the first one.
    service.handle('claude-code', hook('Stop'), task.id)
    expect(service.activity()).toEqual({})
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

  it('moves a task through a run: prompt, question, answer, end of turn without a report', () => {
    const task = tasks.createTask({ title: 'Fix it' })
    const step = (name: string) => service.handle('claude-code', hook(name), task.id)

    expect(step('UserPromptSubmit').applied).toEqual(['task.readied', 'agent.started'])
    expect(tasks.getTask(task.id).state).toBe('running')

    step('PermissionRequest')
    expect(tasks.getTask(task.id).state).toBe('needs-input')

    step('PostToolUse')
    expect(tasks.getTask(task.id).state).toBe('running')

    // The agent did not run `natamux done`, so the turn end waits for you.
    step('Stop')
    expect(tasks.getTask(task.id).state).toBe('needs-input')
  })

  it('gives an agent in a shell that no stage uses a state of its own, and leaves the task alone', () => {
    const task = tasks.createTask({ title: 'Fix it' })
    tasks.recordWorkspace(task.id, { workspace: '/work/T-0001', branch: 'b' })
    const shells = new AgentHookService(tasks, {
      laneFor: (current, terminal) =>
        terminal === '2' ? null : { stage: current.stage, state: current.state },
      takeReport: () => false,
    })
    const events = tasks.listEvents(task.id).length

    const prompt = shells.handle('cursor', { hook_event_name: 'beforeSubmitPrompt' }, task.id, '2')
    expect(prompt).toEqual({ taskId: task.id, applied: [] })
    expect(shells.shellState(task.id, '2')).toBe('running')
    expect(shells.activity()[task.id]?.['2']?.text).toBe('Reading the prompt')

    // The turn ends: the agent waits for the user, as the task's own agent does.
    shells.handle('cursor', { hook_event_name: 'stop', status: 'completed' }, task.id, '2')
    expect(shells.shellState(task.id, '2')).toBe('needs-input')
    shells.handle('cursor', { hook_event_name: 'stop', status: 'error' }, task.id, '2')
    expect(shells.shellState(task.id, '2')).toBe('failed')

    expect(tasks.listEvents(task.id)).toHaveLength(events)
    expect(tasks.getTask(task.id).state).toBe(task.state)
    expect(shells.shellState(task.id, '1')).toBeUndefined()

    // The agent left the shell: a new one there starts with no state.
    shells.forgetShellsExcept(task.id, ['1'])
    expect(shells.shellState(task.id, '2')).toBeUndefined()
  })

  it('forgets the state of a shell once its hooks reach a stage again', () => {
    const task = tasks.createTask({ title: 'Fix it' })
    tasks.recordWorkspace(task.id, { workspace: '/work/T-0001', branch: 'b' })
    let onStage = false
    const shells = new AgentHookService(tasks, {
      laneFor: (current) => (onStage ? { stage: current.stage, state: current.state } : null),
      takeReport: () => false,
    })
    shells.handle('cursor', { hook_event_name: 'beforeSubmitPrompt' }, task.id, '6')
    expect(shells.shellState(task.id, '6')).toBe('running')

    // The task came back to the stage of Shell 6.
    onStage = true
    shells.handle('cursor', { hook_event_name: 'beforeSubmitPrompt' }, task.id, '6')
    expect(shells.shellState(task.id, '6')).toBeUndefined()
  })

  it('drops the turn end that follows a report, and names the stage of the terminal', () => {
    const task = tasks.createTask({ title: 'Fix it' })
    tasks.recordWorkspace(task.id, { workspace: '/work/T-0001', branch: 'b' })
    const seen: (string | undefined)[] = []
    const reporting = new AgentHookService(tasks, {
      laneFor: (current, terminal) => {
        seen.push(terminal)
        return { stage: current.stage, state: current.state }
      },
      takeReport: (_taskId, terminal) => terminal === '2',
    })
    reporting.handle('claude-code', hook('UserPromptSubmit'), task.id, '2')
    expect(tasks.listEvents(task.id).at(-1)?.event.payload).toMatchObject({ stage: 'implement' })

    expect(reporting.handle('claude-code', hook('Stop'), task.id, '2').applied).toEqual([])
    expect(tasks.getTask(task.id).state).toBe('running')
    expect(seen).toEqual(['2', '2'])
  })

  describe('a turn that ends with background work', () => {
    const background = {
      background_tasks: [{ id: 't1', type: 'shell', status: 'running', command: 'pnpm test' }],
    }

    it('keeps the task running and says what the agent waits for', () => {
      const task = tasks.createTask({ title: 'Fix it' })
      service.handle('claude-code', hook('UserPromptSubmit'), task.id)

      const result = service.handle('claude-code', hook('Stop', background), task.id)

      expect(result.applied).toEqual(['agent.waiting'])
      expect(tasks.getTask(task.id).state).toBe('running')
      expect(tasks.getTask(task.id).backgroundSince).not.toBeNull()
      expect(service.activity()[task.id]?.['1']?.text).toBe('Waiting for Bash: pnpm test')
      expect(tasks.listEvents(task.id).at(-1)?.event.payload).toMatchObject({
        waitingFor: ['Bash: pnpm test'],
        stage: 'implement',
      })
    })

    it('goes on as before when the agent comes back and then really stops', () => {
      const task = tasks.createTask({ title: 'Fix it' })
      service.handle('claude-code', hook('UserPromptSubmit'), task.id)
      service.handle('claude-code', hook('Stop', background), task.id)

      service.handle(
        'claude-code',
        hook('PreToolUse', { tool_name: 'Bash', tool_input: { command: 'gh pr checks' } }),
        task.id,
      )
      expect(service.activity()[task.id]?.['1']?.text).toBe('Bash: gh pr checks')

      expect(service.handle('claude-code', hook('Stop'), task.id).applied).toEqual([
        'agent.finished',
      ])
      expect(tasks.getTask(task.id).state).toBe('needs-input')
      expect(tasks.getTask(task.id).backgroundSince).toBeNull()
      expect(service.activity()).toEqual({})
    })

    it('takes a task that waits for the user back to running', () => {
      const task = tasks.createTask({ title: 'Fix it' })
      service.handle('claude-code', hook('UserPromptSubmit'), task.id)
      service.handle('claude-code', hook('PermissionRequest'), task.id)

      expect(service.handle('claude-code', hook('Stop', background), task.id).applied).toEqual([
        'agent.resumed',
        'agent.waiting',
      ])
      expect(tasks.getTask(task.id).state).toBe('running')
      expect(tasks.getTask(task.id).backgroundSince).not.toBeNull()
    })

    it('still drops the turn end that follows a report', () => {
      const task = tasks.createTask({ title: 'Fix it' })
      const reporting = new AgentHookService(tasks, {
        laneFor: (current) => ({ stage: current.stage, state: current.state }),
        takeReport: () => true,
      })
      reporting.handle('claude-code', hook('UserPromptSubmit'), task.id)

      expect(reporting.handle('claude-code', hook('Stop', background), task.id).applied).toEqual([])
      expect(tasks.listEvents(task.id).map((entry) => entry.event.type)).not.toContain(
        'agent.waiting',
      )
      expect(tasks.getTask(task.id).backgroundSince).toBeNull()
      expect(reporting.activity()).toEqual({})
    })

    it('forgets only the line of the terminal that reported', () => {
      const task = tasks.createTask({ title: 'Fix it' })
      tasks.recordWorkspace(task.id, { workspace: `/work/${task.id}`, branch: 'b' })
      const reporting = new AgentHookService(tasks, {
        laneFor: (current) => ({ stage: current.stage, state: current.state }),
        takeReport: (_taskId, terminal) => terminal === '2',
      })
      const edit = (file: string) =>
        hook('PreToolUse', { tool_name: 'Edit', tool_input: { file_path: file } })
      reporting.handle('claude-code', edit('src/a.ts'), task.id, '1')
      reporting.handle('claude-code', edit('test-spec.md'), task.id, '2')

      reporting.handle('claude-code', hook('Stop', background), task.id, '2')

      expect(Object.keys(reporting.activity()[task.id] ?? {})).toEqual(['1'])
    })

    const types = (taskId: string) => tasks.listEvents(taskId).map((entry) => entry.event.type)

    function runningTask() {
      const task = tasks.createTask({ title: 'Fix it' })
      service.handle('claude-code', hook('UserPromptSubmit'), task.id)
      return task
    }

    it('logs each paused turn, and shows the newest list of work', () => {
      const task = runningTask()
      service.handle('claude-code', hook('Stop', background), task.id)

      const next = service.handle(
        'claude-code',
        hook('Stop', { session_crons: [{ id: 'c1', prompt: 'check CI' }] }),
        task.id,
      )

      expect(next.applied).toEqual(['agent.waiting'])
      expect(types(task.id).slice(-2)).toEqual(['agent.waiting', 'agent.waiting'])
      expect(tasks.getTask(task.id).state).toBe('running')
      expect(service.activity()[task.id]?.['1']?.text).toBe(
        'Waiting for scheduled prompt: check CI',
      )
    })

    it('takes a task that waits for the user back to running for a scheduled prompt too', () => {
      const task = runningTask()
      service.handle('claude-code', hook('PermissionRequest'), task.id)

      const result = service.handle(
        'claude-code',
        hook('Stop', { session_crons: [{ id: 'c1', prompt: 'check CI' }] }),
        task.id,
      )

      expect(result.applied).toEqual(['agent.resumed', 'agent.waiting'])
      expect(tasks.getTask(task.id).state).toBe('running')
    })

    it('ends the turn as before for a task that waits for the user, with no background work', () => {
      const task = runningTask()
      service.handle('claude-code', hook('PermissionRequest'), task.id)

      const result = service.handle('claude-code', hook('Stop', { background_tasks: [] }), task.id)

      expect(result.applied).toEqual(['agent.resumed', 'agent.finished'])
      expect(tasks.getTask(task.id).state).toBe('needs-input')
      expect(types(task.id)).not.toContain('agent.waiting')
    })

    it('pauses a later turn as usual once the report was used', () => {
      const task = tasks.createTask({ title: 'Fix it' })
      let reported = true
      const reporting = new AgentHookService(tasks, {
        laneFor: (current) => ({ stage: current.stage, state: current.state }),
        takeReport: () => {
          const taken = reported
          reported = false
          return taken
        },
      })
      reporting.handle('claude-code', hook('UserPromptSubmit'), task.id)
      expect(reporting.handle('claude-code', hook('Stop', background), task.id).applied).toEqual([])

      expect(reporting.handle('claude-code', hook('Stop', background), task.id).applied).toEqual([
        'agent.waiting',
      ])
    })

    it('shows the new work when a scheduled prompt wakes the agent', () => {
      const task = runningTask()
      service.handle('claude-code', hook('Stop', background), task.id)

      service.handle('claude-code', hook('UserPromptSubmit'), task.id)
      expect(service.activity()[task.id]?.['1']?.text).toBe('Reading the prompt')
      service.handle(
        'claude-code',
        hook('PreToolUse', { tool_name: 'Edit', tool_input: { file_path: 'src/a.ts' } }),
        task.id,
      )

      expect(service.activity()[task.id]?.['1']?.text).toBe('Edit src/a.ts')
      expect(tasks.getTask(task.id).state).toBe('running')
      expect(tasks.getTask(task.id).backgroundSince).toBeNull()
    })

    it.each([
      ['StopFailure', {}, 'agent.failed', 'failed'],
      ['PermissionRequest', {}, 'agent.asked', 'needs-input'],
      ['Notification', { notification_type: 'agent_needs_input' }, 'agent.asked', 'needs-input'],
    ])('clears the wait on %s and acts as before', (name, extra, event, state) => {
      const task = runningTask()
      service.handle('claude-code', hook('Stop', background), task.id)

      const result = service.handle('claude-code', hook(name, extra), task.id)

      expect(result.applied).toEqual([event])
      expect(tasks.getTask(task.id).state).toBe(state)
      expect(tasks.getTask(task.id).backgroundSince).toBeNull()
      expect(service.activity()).toEqual({})
    })
  })

  // Natamux terminals run in the task's workspace only.
  describe('after the workspace was removed', () => {
    const lateHooks = [
      hook('UserPromptSubmit'),
      hook('PreToolUse', { tool_name: 'Bash', tool_input: { command: 'ls' } }),
      hook('PermissionRequest'),
      hook('Stop'),
      hook('StopFailure'),
    ]

    // A task whose agent ran in terminal 1 until its workspace was removed.
    function removedTask(state: 'running' | 'needs-input', title = 'Fix it') {
      const task = tasks.createTask({ title })
      tasks.recordWorkspace(task.id, { workspace: `/work/${task.id}`, branch: 'b' })
      service.handle('claude-code', hook('UserPromptSubmit'), task.id, '1')
      if (state === 'needs-input') {
        service.handle('claude-code', hook('PermissionRequest'), task.id, '1')
      }
      tasks.recordWorkspace(task.id, { workspace: null, branch: 'b' })
      return tasks.getTask(task.id)
    }

    it.each(['running', 'needs-input'] as const)(
      'ignores every late hook from its terminal (agent was %s)',
      (state) => {
        const before = removedTask(state)
        const events = tasks.listEvents(before.id).length

        for (const late of lateHooks) {
          expect(service.handle('claude-code', late, before.id, '1')).toEqual({
            taskId: null,
            applied: [],
          })
        }

        expect(tasks.getTask(before.id)).toEqual(before)
        expect(tasks.listEvents(before.id)).toHaveLength(events)
        expect(service.activity()).toEqual({})
      },
    )

    it('does not touch another task', () => {
      const removed = removedTask('running')
      const other = tasks.createTask({ title: 'Other' })
      tasks.recordWorkspace(other.id, { workspace: `/work/${other.id}`, branch: 'c' })
      service.handle('claude-code', hook('UserPromptSubmit'), other.id, '1')

      service.handle('claude-code', hook('Stop'), removed.id, '1')

      expect(tasks.getTask(other.id).state).toBe('running')
      expect(service.handle('claude-code', hook('Stop'), other.id, '1').applied).toEqual([
        'agent.finished',
      ])
    })

    it('still ignores them after a restart', () => {
      const before = removedTask('running')
      const restarted = new AgentHookService(tasks)

      expect(restarted.handle('claude-code', hook('Stop'), before.id, '1').applied).toEqual([])
      expect(tasks.getTask(before.id)).toEqual(before)
    })

    it('follows the agent in a new workspace as usual', () => {
      const before = removedTask('running')
      tasks.applyEvent(before.id, {
        source: 'system',
        type: 'agent.stopped',
        payload: { stage: before.stage },
      })
      tasks.recordWorkspace(before.id, { workspace: `/work/${before.id}`, branch: 'b' })

      const step = (name: string) => service.handle('claude-code', hook(name), before.id, '1')
      expect(step('UserPromptSubmit').applied).toEqual(['agent.started'])
      expect(step('Stop').applied).toEqual(['agent.finished'])
      expect(tasks.getTask(before.id).state).toBe('needs-input')
    })

    it('answers a hook with an unknown task as before', () => {
      expect(service.handle('claude-code', hook('Stop'), 'T-0099', '1')).toEqual({
        taskId: null,
        applied: [],
      })
    })
  })

  it('still follows an agent outside Natamux terminals that names a task with no workspace', () => {
    const task = tasks.createTask({ title: 'Fix it' })

    expect(service.handle('claude-code', hook('UserPromptSubmit'), task.id).applied).toEqual([
      'task.readied',
      'agent.started',
    ])
    expect(service.handle('claude-code', hook('PermissionRequest'), task.id).applied).toEqual([
      'agent.asked',
    ])
    expect(tasks.getTask(task.id).state).toBe('needs-input')
  })

  it('records the events as coming from the agent', () => {
    const task = tasks.createTask({ title: 'Fix it' })
    service.handle('claude-code', hook('UserPromptSubmit'), task.id)

    const sources = tasks.listEvents(task.id).map((entry) => entry.event.source)
    expect(sources).toEqual(['system', 'agent', 'agent'])
  })

  it('does nothing, and logs nothing, when the hook changes nothing', () => {
    const task = tasks.createTask({ title: 'Fix it' })
    const result = service.handle('claude-code', hook('Stop'), task.id)

    expect(result).toEqual({ taskId: task.id, applied: [] })
    expect(tasks.listEvents(task.id)).toHaveLength(1)
  })

  it('ignores a hook the adapter does not use, even when a task matches', () => {
    const task = tasks.createTask({ title: 'Fix it' })
    expect(service.handle('claude-code', hook('SessionEnd'), task.id)).toEqual({
      taskId: null,
      applied: [],
    })
  })

  it('reports that no task was found', () => {
    expect(service.handle('claude-code', hook('Stop', { cwd: '/elsewhere' }), undefined)).toEqual({
      taskId: null,
      applied: [],
    })
  })

  it('throws UnknownAgentError, naming the known agents, for an agent without an adapter', () => {
    expect(() => service.handle('nope', hook('Stop'), undefined)).toThrow(UnknownAgentError)
    expect(() => service.handle('nope', hook('Stop'), undefined)).toThrow(/claude-code/)
  })

  it('throws for input that is not a hook of the agent', () => {
    expect(() => service.handle('claude-code', { cwd: '/x' }, undefined)).toThrow()
  })
})

describe('AgentHookService, sign of life', () => {
  let root: string
  let index: TaskIndex
  let clock: Date
  let tasks: TaskService
  let service: AgentHookService

  const later = (seconds: number) => {
    clock = new Date(clock.getTime() + seconds * 1000)
  }

  const activityEvents = (taskId: string) =>
    tasks.listEvents(taskId).filter((entry) => entry.event.type === 'agent.active')

  beforeEach(() => {
    root = mkdtempSync(join(tmpdir(), 'natamux-activity-'))
    const dataDir = join(root, 'data')
    mkdirSync(dataDir)
    index = new TaskIndex(':memory:')
    clock = new Date('2026-10-08T09:00:00.000Z')
    tasks = new TaskService({ dataDir, index, bus: new EventBus(), now: () => clock })
    service = new AgentHookService(tasks)
  })

  afterEach(() => {
    index.close()
    rmSync(root, { recursive: true, force: true })
  })

  function runningTask() {
    const task = tasks.createTask({ title: 'Fix it' })
    service.handle('claude-code', { hook_event_name: 'UserPromptSubmit' }, task.id)
    return task
  }

  it('records a sign of life when a hook of a running task changes nothing', () => {
    const task = runningTask()
    later(45)

    service.handle('claude-code', { hook_event_name: 'PostToolUse' }, task.id)

    expect(activityEvents(task.id)).toHaveLength(1)
    expect(tasks.getTask(task.id).updatedAt).toBe('2026-10-08T09:00:45.000Z')
  })

  it('does not fill the log when hooks come quickly one after the other', () => {
    const task = runningTask()
    later(45)
    service.handle('claude-code', { hook_event_name: 'PostToolUse' }, task.id)

    for (let step = 0; step < 5; step += 1) {
      later(2)
      service.handle('claude-code', { hook_event_name: 'PostToolUse' }, task.id)
    }

    expect(activityEvents(task.id)).toHaveLength(1)
  })

  it('keeps the time the wait for background work began', () => {
    const task = runningTask()
    const paused = {
      hook_event_name: 'Stop',
      session_crons: [{ id: 'c1', prompt: 'check CI' }],
    }
    later(10)
    service.handle('claude-code', paused, task.id)
    later(300)
    service.handle('claude-code', paused, task.id)

    expect(tasks.getTask(task.id).backgroundSince).toBe('2026-10-08T09:00:10.000Z')
  })

  it('ends the wait at the first sign of life, even soon after it began', () => {
    const task = runningTask()
    service.handle(
      'claude-code',
      { hook_event_name: 'Stop', background_tasks: [{ type: 'shell', command: 'pnpm test' }] },
      task.id,
    )
    later(2)

    service.handle('claude-code', { hook_event_name: 'PostToolUse' }, task.id)

    expect(tasks.getTask(task.id).backgroundSince).toBeNull()
    expect(tasks.getTask(task.id).updatedAt).toBe('2026-10-08T09:00:02.000Z')
  })

  it('records nothing for a hook the adapter does not use, or for a task that is not running', () => {
    const task = tasks.createTask({ title: 'Fix it' })
    later(3600)

    service.handle('claude-code', { hook_event_name: 'Stop' }, task.id)
    service.handle('claude-code', { hook_event_name: 'SessionEnd' }, task.id)

    expect(activityEvents(task.id)).toHaveLength(0)
  })
})

type ActivityMessage = Extract<DaemonMessage, { kind: 'activity.changed' }>

describe('AgentHookService, pushed to the dashboards', () => {
  let root: string
  let index: TaskIndex
  let tasks: TaskService
  let bus: EventBus
  let sent: DaemonMessage[]
  let touched: string[]

  const activityMessages = () =>
    sent.filter((message): message is ActivityMessage => message.kind === 'activity.changed')
  const edit = (path: string) => ({
    hook_event_name: 'PreToolUse',
    tool_name: 'Edit',
    tool_input: { file_path: path },
  })

  beforeEach(() => {
    root = mkdtempSync(join(tmpdir(), 'natamux-push-'))
    const dataDir = join(root, 'data')
    mkdirSync(dataDir)
    index = new TaskIndex(':memory:')
    bus = new EventBus()
    tasks = new TaskService({ dataDir, index, bus })
    sent = []
    touched = []
    bus.subscribe((message) => sent.push(message))
  })

  afterEach(() => {
    index.close()
    rmSync(root, { recursive: true, force: true })
  })

  function hooks(stages?: ConstructorParameters<typeof AgentHookService>[1]): AgentHookService {
    return new AgentHookService(tasks, stages, {
      bus,
      workspaceChanged: (taskId) => touched.push(taskId),
    })
  }

  it('sends the whole activity of the task on each change, and {} when it is gone', () => {
    const service = hooks()
    const task = tasks.createTask({ title: 'Fix it' })
    tasks.recordWorkspace(task.id, { workspace: '/work/T-0001', branch: 'b' })

    service.handle('claude-code', edit('a.ts'), task.id, '1')
    service.handle('claude-code', edit('b.ts'), task.id, '2')
    service.handle('claude-code', { hook_event_name: 'Stop' }, task.id, '2')
    service.handle('claude-code', { hook_event_name: 'Stop' }, task.id, '1')

    const messages = activityMessages()
    expect(messages.map((message) => message.taskId)).toEqual([task.id, task.id, task.id, task.id])
    const texts = messages.map((message) =>
      Object.fromEntries(Object.entries(message.activity).map(([name, line]) => [name, line.text])),
    )
    expect(texts).toEqual([
      { '1': 'Edit a.ts' },
      { '1': 'Edit a.ts', '2': 'Edit b.ts' },
      { '1': 'Edit a.ts' },
      {},
    ])
  })

  it('sends nothing when the activity did not change', () => {
    const service = hooks()
    const task = tasks.createTask({ title: 'Fix it' })

    // No line to clear, and no line to show.
    service.handle('claude-code', { hook_event_name: 'Stop' }, task.id)
    service.handle('claude-code', { hook_event_name: 'SessionEnd' }, task.id)

    expect(activityMessages()).toEqual([])
  })

  it('sends the cleared activity when a turn ends after a report', () => {
    const task = tasks.createTask({ title: 'Fix it' })
    tasks.recordWorkspace(task.id, { workspace: '/work/T-0001', branch: 'b' })
    const service = hooks({
      laneFor: (current) => ({ stage: current.stage, state: current.state }),
      takeReport: () => true,
    })
    service.handle('claude-code', edit('a.ts'), task.id, '1')

    service.handle('claude-code', { hook_event_name: 'Stop' }, task.id, '1')

    const last = activityMessages().at(-1)
    expect(last).toEqual({ kind: 'activity.changed', taskId: task.id, activity: {} })
  })

  it('sends the cleared activity when a hook comes after the workspace was removed', () => {
    const service = hooks()
    const task = tasks.createTask({ title: 'Fix it' })
    tasks.recordWorkspace(task.id, { workspace: '/work/T-0001', branch: 'b' })
    service.handle('claude-code', edit('a.ts'), task.id, '1')
    tasks.recordWorkspace(task.id, { workspace: null, branch: 'b' })

    service.handle('claude-code', edit('b.ts'), task.id, '1')
    service.handle('claude-code', edit('c.ts'), task.id, '1')

    expect(activityMessages().slice(1)).toEqual([
      { kind: 'activity.changed', taskId: task.id, activity: {} },
    ])
  })

  it('tells that the workspace may have changed after a tool ran and at the end of a turn', () => {
    const service = hooks()
    const task = tasks.createTask({ title: 'Fix it' })

    service.handle('claude-code', edit('a.ts'), task.id)
    service.handle('claude-code', { hook_event_name: 'PostToolUse' }, task.id)
    service.handle('claude-code', { hook_event_name: 'Stop' }, task.id)
    service.handle('claude-code', { hook_event_name: 'StopFailure' }, task.id)
    service.handle('claude-code', { hook_event_name: 'Stop', cwd: '/elsewhere' }, undefined)

    expect(touched).toEqual([task.id, task.id, task.id])
  })

  it.each([
    ['claude-code', { hook_event_name: 'UserPromptSubmit' }],
    ['claude-code', { hook_event_name: 'PreToolUse', tool_name: 'Bash' }],
    ['claude-code', { hook_event_name: 'PermissionRequest' }],
    ['claude-code', { hook_event_name: 'Notification', notification_type: 'permission_prompt' }],
    ['cursor', { hook_event_name: 'preToolUse' }],
    ['cursor', { hook_event_name: 'beforeShellExecution', command: 'ls' }],
    ['gemini', { hook_event_name: 'BeforeTool', tool_name: 'read_file' }],
  ])('does not tell for a %s hook that changes no file: %j', (agent, raw) => {
    const service = hooks()
    const task = tasks.createTask({ title: 'Fix it' })

    service.handle(agent, raw, task.id)

    expect(touched).toEqual([])
  })

  it.each([
    ['cursor', { hook_event_name: 'afterFileEdit' }],
    ['cursor', { hook_event_name: 'afterShellExecution' }],
    ['gemini', { hook_event_name: 'AfterTool', tool_name: 'write_file' }],
    ['codex', { hook_event_name: 'PostToolUse' }],
  ])('tells for a %s hook after a tool ran: %j', (agent, raw) => {
    const service = hooks()
    const task = tasks.createTask({ title: 'Fix it' })

    service.handle(agent, raw, task.id)

    expect(touched).toEqual([task.id])
  })

  it('sends shells.changed when a hook changes the state of an agent in a shell', () => {
    const task = tasks.createTask({ title: 'Fix it' })
    tasks.recordWorkspace(task.id, { workspace: '/work/T-0001', branch: 'b' })
    const service = hooks({
      laneFor: (current, terminal) =>
        terminal === '2' ? null : { stage: current.stage, state: current.state },
      takeReport: () => false,
    })
    const shellMessages = () => sent.filter((message) => message.kind === 'shells.changed')
    const changed = { kind: 'shells.changed', taskId: task.id }

    service.handle('claude-code', { hook_event_name: 'UserPromptSubmit' }, task.id, '2')
    expect(shellMessages()).toEqual([changed])

    // Still running: nothing new to show.
    service.handle('claude-code', edit('a.ts'), task.id, '2')
    service.handle('claude-code', { hook_event_name: 'PostToolUse' }, task.id, '2')
    expect(shellMessages()).toEqual([changed])

    service.handle('claude-code', { hook_event_name: 'Stop' }, task.id, '2')
    expect(shellMessages()).toEqual([changed, changed])

    // The agent of a stage shows the state of the task, which comes with the task.
    service.handle('claude-code', { hook_event_name: 'UserPromptSubmit' }, task.id, '1')
    expect(shellMessages()).toEqual([changed, changed])
  })

  it('sends no shells.changed for a shell whose agent the read of the shells did not see', () => {
    const task = tasks.createTask({ title: 'Fix it' })
    tasks.recordWorkspace(task.id, { workspace: '/work/T-0001', branch: 'b' })
    const service = hooks({ laneFor: () => null, takeReport: () => false })
    const shellMessages = () => sent.filter((message) => message.kind === 'shells.changed')
    const prompt = { hook_event_name: 'UserPromptSubmit' }
    service.handle('claude-code', prompt, task.id, '2')
    expect(shellMessages()).toHaveLength(1)

    // Each read of the shells drops the state, as no agent shows there (a shell on Windows).
    for (let step = 0; step < 3; step += 1) {
      service.forgetShellsExcept(task.id, [])
      service.handle('claude-code', prompt, task.id, '2')
    }
    expect(shellMessages()).toHaveLength(1)
    expect(service.shellState(task.id, '2')).toBe('running')

    // Once a read shows an agent there, its next state is sent again.
    service.forgetShellsExcept(task.id, ['2'])
    service.handle('claude-code', { hook_event_name: 'Stop' }, task.id, '2')
    expect(shellMessages()).toHaveLength(2)
  })

  it('sends nothing for a hook of no known task', () => {
    const service = hooks()

    service.handle('claude-code', edit('a.ts'), 'T-0099')

    expect(activityMessages()).toEqual([])
    expect(touched).toEqual([])
  })

  it('sends the same line again for the same hook, which the dashboard just sets again', () => {
    const service = hooks()
    const task = tasks.createTask({ title: 'Fix it' })
    tasks.recordWorkspace(task.id, { workspace: '/work/T-0001', branch: 'b' })

    service.handle('claude-code', edit('a.ts'), task.id, '1')
    service.handle('claude-code', edit('a.ts'), task.id, '1')

    const texts = activityMessages().map((message) => message.activity['1']?.text)
    expect(texts).toEqual(['Edit a.ts', 'Edit a.ts'])
  })
})
