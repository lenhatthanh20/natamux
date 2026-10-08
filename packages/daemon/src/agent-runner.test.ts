import { mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { UnknownAgentError } from './agent-hook-service.js'
import { AgentRunner, firstPrompt, quoteArgument } from './agent-runner.js'
import { EventBus } from './event-bus.js'
import { TaskIndex } from './task-index.js'
import { TaskService } from './task-service.js'
import { type FakePty, fakeSpawner } from './terminal-fake.fixture.js'
import { TerminalService } from './terminal-service.js'
import { WorkspaceConflictError } from './workspace-service.js'

let dataDir: string
let index: TaskIndex
let tasks: TaskService
let ptys: FakePty[]
let runner: AgentRunner

beforeEach(() => {
  dataDir = mkdtempSync(join(tmpdir(), 'helmsman-agents-'))
  index = new TaskIndex(':memory:')
  tasks = new TaskService({ dataDir, index, bus: new EventBus() })
  const spawner = fakeSpawner()
  ptys = spawner.ptys
  const terminals = new TerminalService({
    spawn: spawner.spawn,
    workspaceOf: (taskId) => {
      const { workspace } = tasks.getTask(taskId)
      if (workspace === null) {
        throw new WorkspaceConflictError(`${taskId} has no workspace`)
      }
      return workspace
    },
  })
  runner = new AgentRunner({
    tasks,
    terminals,
    claudeSettingsPath: '/data/claude code.json',
    platform: 'linux',
  })
})

afterEach(() => {
  index.close()
  rmSync(dataDir, { recursive: true, force: true })
})

function taskWithWorkspace(title = 'Fix the bug', description = '') {
  const task = tasks.createTask({ title, description })
  return tasks.recordWorkspace(task.id, { workspace: '/work/T-0001', branch: 'helmsman/t-0001' })
}

describe('quoteArgument', () => {
  it.each([
    ['plain', 'linux', "'plain'"],
    ["it's", 'linux', "'it'\\''s'"],
    ['$HOME `x` "y"', 'darwin', "'$HOME `x` \"y\"'"],
    ["it's", 'win32', "'it''s'"],
  ] as const)('quotes %j for %s', (value, platform, quoted) => {
    expect(quoteArgument(value, platform)).toBe(quoted)
  })
})

describe('firstPrompt', () => {
  it('joins the title and the description on one line', () => {
    expect(firstPrompt({ title: 'Fix it', description: 'Line one.\n\nLine  two.' })).toBe(
      'Fix it - Line one. Line two.',
    )
    expect(firstPrompt({ title: 'Fix it', description: '  ' })).toBe('Fix it')
  })
})

describe('AgentRunner', () => {
  it('types the Claude Code command with the hook settings and the first prompt', () => {
    const task = taskWithWorkspace('Fix the bug', "Don't break it.")

    const updated = runner.start(task.id, 'claude-code')

    expect(ptys).toHaveLength(1)
    expect(ptys[0]?.options.cwd).toBe('/work/T-0001')
    expect(ptys[0]?.written).toEqual([
      "claude --settings '/data/claude code.json' 'Fix the bug - Don'\\''t break it.'\r",
    ])
    expect(updated.agent).toBe('claude-code')
    expect(tasks.listEvents(task.id).at(-1)?.event).toMatchObject({
      type: 'agent.launched',
      payload: { agent: 'claude-code' },
    })
  })

  it('uses the prompt it is given, and the shell that is open', () => {
    const task = taskWithWorkspace()
    runner.start(task.id, 'claude-code', 'Only this')
    runner.start(task.id, 'claude-code', '  ')

    expect(ptys).toHaveLength(1)
    expect(ptys[0]?.written).toEqual([
      "claude --settings '/data/claude code.json' 'Only this'\r",
      "claude --settings '/data/claude code.json'\r",
    ])
  })

  it('refuses an agent it cannot start, and a task without a workspace', () => {
    const task = taskWithWorkspace()
    expect(() => runner.start(task.id, 'unknown')).toThrow(UnknownAgentError)

    const bare = tasks.createTask({ title: 'No workspace' })
    expect(() => runner.start(bare.id, 'claude-code')).toThrow(WorkspaceConflictError)
    expect(tasks.getTask(bare.id).agent).toBeNull()
  })
})
