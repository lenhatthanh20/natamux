import { execFileSync } from 'node:child_process'
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import {
  codexHookArgs,
  copilotHookFile,
  cursorHookFile,
  geminiHookSettings,
  type LaunchOptions,
  launchCommand,
  prepareWorkspace,
} from './agent-launch.js'

const base: LaunchOptions = {
  program: 'x',
  extraArgs: '',
  prompt: 'Fix it',
  platform: 'linux',
  claudeSettingsPath: '/data/claude.json',
  geminiSettingsPath: '/data/gemini.json',
}

describe('launchCommand', () => {
  it('starts each agent interactive, with its first prompt and its hooks', () => {
    expect(launchCommand('claude-code', { ...base, program: 'claude' })).toBe(
      "claude --settings '/data/claude.json' 'Fix it'",
    )
    expect(
      launchCommand('copilot', { ...base, program: 'copilot', extraArgs: '--allow-all-tools' }),
    ).toBe("copilot --allow-all-tools -i 'Fix it'")
    expect(launchCommand('cursor', { ...base, program: 'cursor-agent' })).toBe(
      "cursor-agent 'Fix it'",
    )
    expect(launchCommand('gemini', { ...base, program: 'gemini' })).toBe(
      "GEMINI_CLI_SYSTEM_SETTINGS_PATH='/data/gemini.json' gemini -i 'Fix it'",
    )
    const codex = launchCommand('codex', { ...base, program: 'codex' })
    expect(codex.startsWith('codex --dangerously-bypass-hook-trust -c ')).toBe(true)
    expect(codex).toContain(
      `'hooks.Stop=[{hooks=[{type="command",command="natamux hook codex",timeout=5}]}]'`,
    )
    expect(codex.endsWith(" 'Fix it'")).toBe(true)
  })

  it('sets the Gemini settings in PowerShell on Windows, and leaves the prompt out when empty', () => {
    expect(
      launchCommand('gemini', { ...base, program: 'gemini', platform: 'win32', prompt: '' }),
    ).toBe("$env:GEMINI_CLI_SYSTEM_SETTINGS_PATH='/data/gemini.json'; gemini")
  })
})

describe('the hook settings', () => {
  it('send every event Natamux reads to `natamux hook <agent>`', () => {
    expect(codexHookArgs().filter((arg) => arg !== '-c')).toHaveLength(6)
    expect(Object.keys(geminiHookSettings().hooks)).toEqual([
      'BeforeAgent',
      'BeforeTool',
      'AfterTool',
      'Notification',
      'AfterAgent',
    ])
    expect(copilotHookFile().hooks.agentStop).toEqual([
      {
        type: 'command',
        bash: 'natamux hook copilot agentStop',
        powershell: 'natamux hook copilot agentStop',
        timeoutSec: 5,
      },
    ])
    expect(cursorHookFile()).toMatchObject({
      version: 1,
      hooks: { stop: [{ command: 'natamux hook cursor', timeout: 5 }] },
    })
  })
})

describe('prepareWorkspace', () => {
  let repo: string

  beforeEach(() => {
    repo = mkdtempSync(join(tmpdir(), 'natamux-launch-'))
    execFileSync('git', ['init', '-q'], { cwd: repo })
  })

  afterEach(() => {
    rmSync(repo, { recursive: true, force: true })
  })

  it('writes the hook file and keeps it out of git, once', () => {
    expect(prepareWorkspace('copilot', repo)).toBe(true)
    expect(prepareWorkspace('copilot', repo)).toBe(true)
    expect(existsSync(join(repo, '.github/hooks/natamux.json'))).toBe(true)
    const exclude = readFileSync(join(repo, '.git/info/exclude'), 'utf8')
    expect(
      exclude.split('\n').filter((line) => line === '/.github/hooks/natamux.json'),
    ).toHaveLength(1)
    expect(execFileSync('git', ['status', '--porcelain'], { cwd: repo, encoding: 'utf8' })).toBe('')
  })

  it("leaves the user's own Cursor hooks alone", () => {
    mkdirSync(join(repo, '.cursor'))
    writeFileSync(
      join(repo, '.cursor/hooks.json'),
      '{"version":1,"hooks":{"stop":[{"command":"say done"}]}}',
    )
    expect(prepareWorkspace('cursor', repo)).toBe(false)
    expect(readFileSync(join(repo, '.cursor/hooks.json'), 'utf8')).toContain('say done')
  })

  it('needs no file for agents that take their hooks another way', () => {
    expect(prepareWorkspace('codex', repo)).toBe(true)
    expect(existsSync(join(repo, '.codex'))).toBe(false)
  })
})
