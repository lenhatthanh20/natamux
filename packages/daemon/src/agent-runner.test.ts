import { mkdtempSync, readFileSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { UnknownAgentError } from './agent-hook-service.js'
import { AgentRunner, firstPrompt, quoteArgument, TaskWaitingError } from './agent-runner.js'
import { EventBus } from './event-bus.js'
import { TaskIndex } from './task-index.js'
import { TaskService } from './task-service.js'
import { type FakePty, fakeSpawner } from './terminal-fake.fixture.js'
import { TerminalService } from './terminal-service.js'
import { WorkspaceConflictError } from './workspace-service.js'

const size = { cols: 80, rows: 24 }
let dataDir: string
let index: TaskIndex
let tasks: TaskService
let ptys: FakePty[]
let runner: AgentRunner
let terminals: TerminalService

beforeEach(() => {
  dataDir = mkdtempSync(join(tmpdir(), 'helmsman-agents-'))
  index = new TaskIndex(':memory:')
  tasks = new TaskService({ dataDir, index, bus: new EventBus() })
  const spawner = fakeSpawner()
  ptys = spawner.ptys
  terminals = new TerminalService({
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
    launchDir: dataDir,
  })
})

afterEach(() => {
  index.close()
  rmSync(dataDir, { recursive: true, force: true })
})

function script(taskId: string): string {
  return join(dataDir, `agent-launch-${taskId}.sh`)
}

function taskWithWorkspace(title = 'Fix the bug', description = '') {
  const task = tasks.createTask({ title, description })
  return tasks.recordWorkspace(task.id, { workspace: '/work/T-0001', branch: 'helmsman/t-0001' })
}

describe('quoteArgument', () => {
  it.each([
    ['plain', 'linux', "'plain'"],
    ["it's", 'linux', "'it'\\''s'"],
    ['$HOME `x` "y"', 'darwin', '\'$HOME `x` "y"\''],
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
  it('runs a launch script with the hook settings and the first prompt', () => {
    const task = taskWithWorkspace('Fix the bug', "Don't break it.")

    const updated = runner.start(task.id, 'claude-code')

    expect(ptys).toHaveLength(1)
    expect(ptys[0]?.options.cwd).toBe('/work/T-0001')
    expect(ptys[0]?.written).toEqual([`. '${script(task.id)}'\r`])
    expect(readFileSync(script(task.id), 'utf8')).toBe(
      "claude --settings '/data/claude code.json' 'Fix the bug - Don'\\''t break it.'\n",
    )
    expect(updated.agent).toBe('claude-code')
    expect(tasks.listEvents(task.id).at(-1)?.event).toMatchObject({
      type: 'agent.launched',
      payload: { agent: 'claude-code' },
    })
  })

  it('does not show the settings path or the prompt in the terminal', () => {
    const task = taskWithWorkspace('Fix the bug', 'A long secret description.')
    runner.start(task.id, 'claude-code')

    // The fake terminal echoes what is typed, like a real one. This is what a browser sees.
    const seen: string[] = []
    terminals.attach(task.id, { send: (message) => seen.push(JSON.stringify(message)) }, size)

    const text = seen.join('')
    expect(text).toContain('agent-launch-T-0001.sh')
    expect(text).not.toContain('claude code.json')
    expect(text).not.toContain('--settings')
    expect(text).not.toContain('secret')
  })

  it('uses the prompt it is given, and the shell that is open', () => {
    const task = taskWithWorkspace()
    runner.start(task.id, 'claude-code', 'Only this')
    expect(readFileSync(script(task.id), 'utf8')).toBe(
      "claude --settings '/data/claude code.json' 'Only this'\n",
    )
    runner.start(task.id, 'claude-code', '  ')

    expect(ptys).toHaveLength(1)
    expect(ptys[0]?.written).toEqual([`. '${script(task.id)}'\r`, `. '${script(task.id)}'\r`])
    expect(readFileSync(script(task.id), 'utf8')).toBe(
      "claude --settings '/data/claude code.json'\n",
    )
  })

  it('types the command itself on Windows, where scripts may be refused', () => {
    const spawner = fakeSpawner()
    const windows = new AgentRunner({
      tasks,
      terminals: new TerminalService({ spawn: spawner.spawn, workspaceOf: () => '/work/T-0001' }),
      claudeSettingsPath: 'C:\\data\\settings.json',
      platform: 'win32',
      launchDir: dataDir,
    })
    const task = taskWithWorkspace()

    windows.start(task.id, 'claude-code', 'Only this')

    expect(spawner.ptys[0]?.written).toEqual([
      "claude --settings 'C:\\data\\settings.json' 'Only this'\r",
    ])
  })

  it('refuses a task that waits for another one', () => {
    const before = tasks.createTask({ title: 'First things first' })
    const task = taskWithWorkspace()
    tasks.updateTask(task.id, { dependsOn: [before.id] })

    expect(() => runner.start(task.id, 'claude-code')).toThrow(TaskWaitingError)
    expect(ptys).toHaveLength(0)
  })

  it('refuses an agent it cannot start, and a task without a workspace', () => {
    const task = taskWithWorkspace()
    expect(() => runner.start(task.id, 'unknown')).toThrow(UnknownAgentError)

    const bare = tasks.createTask({ title: 'No workspace' })
    expect(() => runner.start(bare.id, 'claude-code')).toThrow(WorkspaceConflictError)
    expect(tasks.getTask(bare.id).agent).toBeNull()
  })
})

describe('GitHub Copilot', () => {
  it('starts copilot in interactive mode with the prompt and the extra words', () => {
    const copilot = new AgentRunner({
      tasks,
      terminals,
      claudeSettingsPath: '/data/claude-code-settings.json',
      copilotArgs: '--allow-all-tools',
      platform: 'linux',
      launchDir: dataDir,
    })
    const task = taskWithWorkspace('Fix the parser')
    copilot.start(task.id, 'copilot', 'Fix it')
    expect(readFileSync(script(task.id), 'utf8')).toBe(
      "copilot --allow-all-tools -i 'Fix it'\n",
    )
    expect(tasks.getTask(task.id).agent).toBe('copilot')
  })
})
