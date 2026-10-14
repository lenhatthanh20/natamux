import { execFileSync } from 'node:child_process'
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { DEFAULT_PROJECT_SETTINGS } from '@natamux/core'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { UnknownAgentError } from './agent-hook-service.js'
import { trustPaths } from './agent-launch.js'
import { AgentRunner, firstPrompt, quoteArgument, TaskWaitingError } from './agent-runner.js'
import { EventBus } from './event-bus.js'
import { ProjectSettingsService } from './project-settings.js'
import { TaskIndex } from './task-index.js'
import { TaskService } from './task-service.js'
import { type FakePty, fakeSpawner } from './terminal-fake.fixture.js'
import { type TerminalFrame, TerminalService } from './terminal-service.js'
import { WorkspaceConflictError } from './workspace-service.js'

// What Windows types: the command between the window titles that tell its start and its end.
const windowsLine = (command: string) =>
  `$Host.UI.RawUI.WindowTitle = 'natamux-agent'; ${command}; $Host.UI.RawUI.WindowTitle = 'natamux-agent-exited'\r`
let dataDir: string
let index: TaskIndex
let tasks: TaskService
let ptys: FakePty[]
let runner: AgentRunner
let terminals: TerminalService

beforeEach(() => {
  dataDir = mkdtempSync(join(tmpdir(), 'natamux-agents-'))
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
    trustPaths: trustPaths({}, dataDir),
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
  return tasks.recordWorkspace(task.id, { workspace: '/work/T-0001', branch: 'natamux/t-0001' })
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
      "claude --settings '/data/claude code.json' --permission-mode auto 'Fix the bug - Don'\\''t break it.'\n",
    )
    expect(updated.agent).toBe('claude-code')
    expect(tasks.listEvents(task.id).at(-1)?.event).toMatchObject({
      type: 'agent.launched',
      payload: { agent: 'claude-code' },
    })
  })

  it('does not show the settings path or the prompt in the terminal', async () => {
    const task = taskWithWorkspace('Fix the bug', 'A long secret description.')
    runner.start(task.id, 'claude-code')

    // The fake terminal echoes what is typed, like a real one. This is what a browser sees.
    const seen: string[] = []
    // Wide enough for the long temp path of macOS and Windows: on a resize, xterm keeps the line
    // with the cursor as it is, so a narrower screen would cut it.
    const wide = { cols: 200, rows: 40 }
    const send = (frame: TerminalFrame) =>
      seen.push(frame instanceof Uint8Array ? Buffer.from(frame).toString() : JSON.stringify(frame))
    terminals.attach(task.id, { send }, wide)
    await terminals.settled(task.id)

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
      "claude --settings '/data/claude code.json' --permission-mode auto 'Only this'\n",
    )
    runner.start(task.id, 'claude-code', '  ')

    expect(ptys).toHaveLength(1)
    expect(ptys[0]?.written).toEqual([`. '${script(task.id)}'\r`, `. '${script(task.id)}'\r`])
    expect(readFileSync(script(task.id), 'utf8')).toBe(
      "claude --settings '/data/claude code.json' --permission-mode auto\n",
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
      windowsLine("claude --settings 'C:\\data\\settings.json' --permission-mode auto 'Only this'"),
    ])
  })

  // PSReadLine takes these bytes as keys: ESC wipes the typed line and Ctrl+C cancels it, so
  // an issue body could run its own command.
  it('types no control bytes of the prompt on Windows', () => {
    const spawner = fakeSpawner()
    const windows = new AgentRunner({
      tasks,
      terminals: new TerminalService({ spawn: spawner.spawn, workspaceOf: () => '/work/T-0001' }),
      claudeSettingsPath: 'C:\\data\\settings.json',
      platform: 'win32',
      launchDir: dataDir,
    })
    const task = taskWithWorkspace()

    windows.start(task.id, 'claude-code', 'a\x1bcalc.exe #\x03b')

    expect(spawner.ptys[0]?.written).toEqual([
      windowsLine(
        "claude --settings 'C:\\data\\settings.json' --permission-mode auto 'acalc.exe #b'",
      ),
    ])
  })

  it('takes escape sequences and control bytes out of the first prompt', () => {
    const task = taskWithWorkspace('Fix\x1b[2K the bug', 'Do\x03 it\x1b]0;title\x07.')

    runner.start(task.id, 'claude-code')

    expect(readFileSync(script(task.id), 'utf8')).toBe(
      "claude --settings '/data/claude code.json' --permission-mode auto 'Fix the bug - Do it.'\n",
    )
  })

  it('puts the natamux command first on the PATH of the agent', () => {
    const withBin = new AgentRunner({
      tasks,
      terminals,
      claudeSettingsPath: '/data/claude code.json',
      platform: 'darwin',
      launchDir: dataDir,
      binDir: "/Apps/Nata'mux/bin",
    })
    const task = taskWithWorkspace()

    withBin.start(task.id, 'claude-code', 'Only this')

    expect(ptys[0]?.written).toEqual([`. '${script(task.id)}'\r`])
    expect(readFileSync(script(task.id), 'utf8')).toBe(
      `PATH='/Apps/Nata'\\''mux/bin':"$PATH" claude --settings '/data/claude code.json' --permission-mode auto 'Only this'\n`,
    )
  })

  it('keeps a bin folder with a space as one PATH entry', () => {
    const withBin = new AgentRunner({
      tasks,
      terminals,
      claudeSettingsPath: '/data/claude code.json',
      platform: 'darwin',
      launchDir: dataDir,
      binDir: '/Users/me/Library/Application Support/Natamux/bin',
    })
    const task = taskWithWorkspace()

    withBin.start(task.id, 'claude-code', 'Only this')

    expect(readFileSync(script(task.id), 'utf8')).toMatch(
      /^PATH='\/Users\/me\/Library\/Application Support\/Natamux\/bin':"\$PATH" claude /,
    )
  })

  // The script is sourced by a real shell here. A fake natamux elsewhere on the PATH must lose.
  it.skipIf(process.platform === 'win32')(
    'runs the app natamux before another one in a real shell',
    () => {
      const appBin = join(dataDir, 'Application Support', 'bin')
      const otherBin = join(dataDir, 'other')
      for (const [dir, name] of [
        [appBin, 'app'],
        [otherBin, 'other'],
      ] as const) {
        mkdirSync(dir, { recursive: true })
        writeFileSync(join(dir, 'natamux'), `#!/bin/sh\necho ${name}\n`, { mode: 0o755 })
      }
      const task = taskWithWorkspace()
      const source = (binDir: string | undefined) => {
        new AgentRunner({
          tasks,
          terminals,
          claudeSettingsPath: '/data/settings.json',
          platform: process.platform,
          launchDir: dataDir,
          binDir,
          programs: { 'claude-code': 'natamux' },
        }).start(task.id, 'claude-code', 'Only this')
        return execFileSync('/bin/sh', ['-c', '. "$1"', 'sh', script(task.id)], {
          encoding: 'utf8',
          env: { PATH: `${otherBin}:/usr/bin:/bin` },
        }).trim()
      }

      expect(source(appBin)).toBe('app')
      // Without a bin folder (a daemon not started by the app) the script still runs.
      expect(source(undefined)).toBe('other')
    },
  )

  it('leaves the PATH alone on Windows, even with a bin folder', () => {
    const spawner = fakeSpawner()
    const windows = new AgentRunner({
      tasks,
      terminals: new TerminalService({ spawn: spawner.spawn, workspaceOf: () => '/work/T-0001' }),
      claudeSettingsPath: 'C:\\data\\settings.json',
      platform: 'win32',
      launchDir: dataDir,
      binDir: 'C:\\Natamux\\bin',
    })
    const task = taskWithWorkspace()

    windows.start(task.id, 'claude-code', 'Only this')

    expect(spawner.ptys[0]?.written).toEqual([
      windowsLine("claude --settings 'C:\\data\\settings.json' --permission-mode auto 'Only this'"),
    ])
  })

  it('reads the permission mode of the project at each start', () => {
    let permissionMode: 'auto' | 'ask' = 'ask'
    const project = new TaskService({
      dataDir,
      index,
      bus: new EventBus(),
      settingsOf: () => ({ ...DEFAULT_PROJECT_SETTINGS, agents: { permissionMode } }),
    })
    const asking = new AgentRunner({
      tasks: project,
      terminals,
      claudeSettingsPath: '/data/claude code.json',
      platform: 'linux',
      launchDir: dataDir,
    })
    const created = project.createTask({ title: 'Fix the bug' })
    const task = project.recordWorkspace(created.id, {
      workspace: '/work/T-0001',
      branch: 'natamux/t-0001',
    })

    asking.start(task.id, 'claude-code', 'Only this')
    expect(readFileSync(script(task.id), 'utf8')).toBe(
      "claude --settings '/data/claude code.json' 'Only this'\n",
    )
    permissionMode = 'auto'
    asking.start(task.id, 'claude-code', 'Only this')
    expect(readFileSync(script(task.id), 'utf8')).toBe(
      "claude --settings '/data/claude code.json' --permission-mode auto 'Only this'\n",
    )
  })

  it('starts Codex in its sandbox without asking when the project sets no mode', () => {
    const task = taskWithWorkspace()
    runner.start(task.id, 'codex', 'Only this')
    expect(
      readFileSync(script(task.id), 'utf8').startsWith(
        'codex --dangerously-bypass-hook-trust --sandbox workspace-write --ask-for-approval never -c ',
      ),
    ).toBe(true)
  })

  it('trusts the task workspace for Codex and Cursor', () => {
    const task = taskWithWorkspace()
    runner.start(task.id, 'codex', 'Only this')
    expect(readFileSync(script(task.id), 'utf8')).toContain(
      ` -c 'projects={"/work/T-0001"={trust_level="trusted"}}' `,
    )
    runner.start(task.id, 'cursor', 'Only this')
    const marker = join(dataDir, '.cursor/projects/work-T-0001/.workspace-trusted')
    expect(JSON.parse(readFileSync(marker, 'utf8')).workspacePath).toBe('/work/T-0001')
    runner.start(task.id, 'gemini', 'Only this')
    expect(readFileSync(script(task.id), 'utf8')).toMatch(/^GEMINI_CLI_TRUST_WORKSPACE=true /)
  })

  it('starts the agent when its trust cannot be written', () => {
    const file = join(dataDir, 'a-file')
    writeFileSync(file, '')
    const broken = new AgentRunner({
      tasks,
      terminals,
      claudeSettingsPath: '/data/claude code.json',
      platform: 'linux',
      launchDir: dataDir,
      trustPaths: trustPaths({}, join(file, 'home')),
    })
    const task = taskWithWorkspace()
    for (const agent of ['cursor', 'copilot'] as const) {
      broken.start(task.id, agent, 'Only this')
      expect(tasks.getTask(task.id).agent).toBe(agent)
    }
    expect(readFileSync(script(task.id), 'utf8')).toBe("copilot --allow-all-tools -i 'Only this'\n")
  })

  it.each(['', ' ', '  \t '])('adds no extra word for NATAMUX_CODEX_ARGS of %j', (words) => {
    const spaced = new AgentRunner({
      tasks,
      terminals,
      claudeSettingsPath: '/data/claude code.json',
      platform: 'linux',
      launchDir: dataDir,
      extraArgs: { codex: words },
    })
    const task = taskWithWorkspace()
    spaced.start(task.id, 'codex', 'Only this')
    expect(readFileSync(script(task.id), 'utf8')).toMatch(/[^ ] 'Only this'\n$/)
  })

  it("starts a stage's own agent with its words after the configured ones, and keeps the task agent", () => {
    const configured = new AgentRunner({
      tasks,
      terminals,
      claudeSettingsPath: '/data/claude code.json',
      platform: 'linux',
      launchDir: dataDir,
      extraArgs: { codex: '--foo' },
    })
    const task = taskWithWorkspace()
    configured.start(task.id, 'claude-code')

    const updated = configured.start(task.id, 'codex', 'Only this', '2', undefined, {
      args: '--model gpt-5',
      stage: 'test-spec',
    })

    expect(readFileSync(join(dataDir, `agent-launch-${task.id}-2.sh`), 'utf8')).toContain(
      "--foo --model gpt-5 'Only this'",
    )
    expect(updated.agent).toBe('claude-code')
    expect(tasks.listEvents(task.id).at(-1)?.event).toMatchObject({
      type: 'agent.launched',
      payload: { agent: 'codex', stage: 'test-spec' },
    })
  })

  it('starts the next agent with the mode saved in .natamux.json', () => {
    // Wired as in main.ts: the tasks read the settings of the repository's file.
    const settings = new ProjectSettingsService({ repos: { pathOf: () => dataDir } })
    const project = new TaskService({
      dataDir,
      index,
      bus: new EventBus(),
      settingsOf: (repo) => settings.get(repo),
    })
    const saved = new AgentRunner({
      tasks: project,
      terminals,
      claudeSettingsPath: '/data/claude code.json',
      platform: 'linux',
      launchDir: dataDir,
    })
    const task = taskWithWorkspace()

    saved.start(task.id, 'claude-code', 'Only this')
    expect(readFileSync(script(task.id), 'utf8')).toBe(
      "claude --settings '/data/claude code.json' --permission-mode auto 'Only this'\n",
    )
    settings.save(null, { agents: { permissionMode: 'ask' } })
    saved.start(task.id, 'claude-code', 'Only this')
    expect(readFileSync(script(task.id), 'utf8')).toBe(
      "claude --settings '/data/claude code.json' 'Only this'\n",
    )
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
      copilotArgs: '--model gpt-5',
      platform: 'linux',
      launchDir: dataDir,
      trustPaths: trustPaths({}, dataDir),
    })
    const task = taskWithWorkspace('Fix the parser')
    copilot.start(task.id, 'copilot', 'Fix it')
    expect(readFileSync(script(task.id), 'utf8')).toBe(
      "copilot --allow-all-tools --model gpt-5 -i 'Fix it'\n",
    )
    expect(tasks.getTask(task.id).agent).toBe('copilot')
  })
})
