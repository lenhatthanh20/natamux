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
    root = mkdtempSync(join(tmpdir(), 'helmsman-hooks-'))
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
    const step = (name: string) => service.handle('claude-code', hook(name), task.id)

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
