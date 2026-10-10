import { mkdirSync, mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { findAdapter, knownAgentIds } from './agent-adapters.js'
import { AgentHookService, UnknownAgentError } from './agent-hook-service.js'
import { EventBus } from './event-bus.js'
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

  it('drops the turn end that follows a report, and names the stage of the terminal', () => {
    const task = tasks.createTask({ title: 'Fix it' })
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

  it('records nothing for a hook the adapter does not use, or for a task that is not running', () => {
    const task = tasks.createTask({ title: 'Fix it' })
    later(3600)

    service.handle('claude-code', { hook_event_name: 'Stop' }, task.id)
    service.handle('claude-code', { hook_event_name: 'SessionEnd' }, task.id)

    expect(activityEvents(task.id)).toHaveLength(0)
  })
})
