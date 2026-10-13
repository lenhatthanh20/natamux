import { execFileSync } from 'node:child_process'
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import {
  AUTO_FLAGS,
  agentOfCommand,
  codexHookArgs,
  copilotHookFile,
  cursorHookFile,
  DEFAULT_PROGRAMS,
  geminiHookSettings,
  LAUNCHABLE_AGENTS,
  type LaunchableAgent,
  type LaunchOptions,
  launchCommand,
  oneShotCommand,
  prepareAgentHooks,
  prepareWorkspace,
  quoteArgument,
} from './agent-launch.js'

const base: LaunchOptions = {
  program: 'x',
  extraArgs: '',
  prompt: 'Fix it',
  platform: 'linux',
  claudeSettingsPath: '/data/claude.json',
  geminiSettingsPath: '/data/gemini.json',
  permissionMode: 'ask',
}

const auto: LaunchOptions = { ...base, permissionMode: 'auto' }

describe('launchCommand', () => {
  it('starts each agent in its auto mode, before the extra words and the prompt', () => {
    expect(launchCommand('claude-code', { ...auto, program: 'claude', extraArgs: '--x' })).toBe(
      "claude --settings '/data/claude.json' --permission-mode auto --x 'Fix it'",
    )
    expect(launchCommand('copilot', { ...auto, program: 'copilot', extraArgs: '--x' })).toBe(
      "copilot --allow-all-tools --x -i 'Fix it'",
    )
    expect(launchCommand('cursor', { ...auto, program: 'cursor-agent', extraArgs: '--x' })).toBe(
      "cursor-agent --force --x 'Fix it'",
    )
    expect(launchCommand('gemini', { ...auto, program: 'gemini', extraArgs: '--x' })).toBe(
      "GEMINI_CLI_SYSTEM_SETTINGS_PATH='/data/gemini.json' gemini --approval-mode yolo --x -i 'Fix it'",
    )
    const codex = launchCommand('codex', { ...auto, program: 'codex', extraArgs: '--x' })
    expect(
      codex.startsWith(
        'codex --dangerously-bypass-hook-trust --sandbox workspace-write --ask-for-approval never -c ',
      ),
    ).toBe(true)
    expect(codex).not.toContain('--full-auto')
    expect(codex.endsWith(" --x 'Fix it'")).toBe(true)
  })

  it('adds the auto mode on Windows and without a prompt', () => {
    expect(launchCommand('claude-code', { ...auto, program: 'claude', prompt: '' })).toBe(
      "claude --settings '/data/claude.json' --permission-mode auto",
    )
    expect(
      launchCommand('claude-code', {
        ...auto,
        program: 'claude',
        platform: 'win32',
        prompt: "it's",
      }),
    ).toBe("claude --settings '/data/claude.json' --permission-mode auto 'it''s'")
    expect(
      launchCommand('gemini', { ...auto, program: 'gemini', platform: 'win32', prompt: '' }),
    ).toBe("$env:GEMINI_CLI_SYSTEM_SETTINGS_PATH='/data/gemini.json'; gemini --approval-mode yolo")
  })

  it('goes on with a Claude Code session after the mode', () => {
    expect(
      launchCommand('claude-code', { ...auto, program: 'claude', prompt: '', session: 's-1' }),
    ).toBe("claude --settings '/data/claude.json' --permission-mode auto --resume 's-1'")
  })

  it('quotes the session, so it cannot run as a command', () => {
    const session = "s'; rm -rf ~; '"
    expect(launchCommand('claude-code', { ...base, program: 'claude', prompt: '', session })).toBe(
      `claude --settings '/data/claude.json' --resume ${quoteArgument(session, 'linux')}`,
    )
    expect(quoteArgument(session, 'linux')).toBe("'s'\\''; rm -rf ~; '\\'''")
  })

  it('starts other agents with the prompt, as only Claude Code goes on with a session', () => {
    for (const agent of LAUNCHABLE_AGENTS.filter((id) => id !== 'claude-code')) {
      const command = launchCommand(agent, { ...base, session: 's-1' })
      expect(command).not.toContain('s-1')
      expect(command).toContain("'Fix it'")
    }
  })

  it.each([
    ['claude-code', '--permission-mode auto'],
    ['codex', '--sandbox workspace-write --ask-for-approval never'],
    ['gemini', '--approval-mode yolo'],
    ['copilot', '--allow-all-tools'],
    ['cursor', '--force'],
  ] as const)('quotes the prompt of %s for PowerShell next to its flag', (agent, flag) => {
    const command = launchCommand(agent, { ...auto, platform: 'win32', prompt: "it's" })
    expect(command).toContain(` ${flag} `)
    expect(command.endsWith(" 'it''s'")).toBe(true)
  })

  it('doubles the typographic single quotes for PowerShell too', () => {
    expect(quoteArgument('Don\u2019t crash; Remove-Item ~\\src', 'win32')).toBe(
      "'Don\u2019\u2019t crash; Remove-Item ~\\src'",
    )
    expect(quoteArgument("a'b\u2018c\u2019d\u201Ae\u201Bf", 'win32')).toBe(
      "'a''b\u2018\u2018c\u2019\u2019d\u201A\u201Ae\u201B\u201Bf'",
    )
    // A POSIX shell has only the ASCII quote.
    expect(quoteArgument('Don\u2019t', 'linux')).toBe("'Don\u2019t'")
  })

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
    expect(codex).not.toContain('--sandbox')
    expect(codex).not.toContain('--ask-for-approval')
    expect(codex).toContain(
      `'hooks.Stop=[{hooks=[{type="command",command="natamux hook codex",timeout=5}]}]'`,
    )
    expect(codex.endsWith(" 'Fix it'")).toBe(true)
  })

  it('adds the Codex sandbox flags once each, in order, and keeps its hooks', () => {
    const prompt = "Don't break 'Application Support' or \"$HOME\""
    const asking = launchCommand('codex', { ...base, program: 'codex', prompt })
    const codex = launchCommand('codex', { ...auto, program: 'codex', prompt })
    expect(codex).toBe(
      asking.replace(
        '--dangerously-bypass-hook-trust ',
        '--dangerously-bypass-hook-trust --sandbox workspace-write --ask-for-approval never ',
      ),
    )
    expect(codex.split(' --sandbox ')).toHaveLength(2)
    expect(codex.split(' --ask-for-approval ')).toHaveLength(2)
    expect(codex.endsWith(` ${quoteArgument(prompt, 'linux')}`)).toBe(true)
  })

  it.each(LAUNCHABLE_AGENTS)('adds no auto flag of any agent to %s in the ask mode', (agent) => {
    const command = launchCommand(agent, { ...base, program: agent })
    for (const flags of Object.values(AUTO_FLAGS)) {
      expect(command).not.toContain(flags.join(' '))
    }
    expect(command).not.toContain('--full-auto')
  })

  it('keeps the Codex hooks in the ask mode', () => {
    const codex = launchCommand('codex', { ...base, program: 'codex' })
    expect(codex).toContain(' --dangerously-bypass-hook-trust ')
    expect(codex.split(' -c ')).toHaveLength(codexHookArgs().length / 2 + 1)
  })

  it('adds no flags for a mode other than auto', () => {
    const mode = 'yolo' as string as LaunchOptions['permissionMode']
    expect(launchCommand('codex', { ...base, program: 'codex', permissionMode: mode })).toBe(
      launchCommand('codex', { ...base, program: 'codex' }),
    )
  })

  it('puts the words of NATAMUX_CODEX_ARGS after the Codex mode flags', () => {
    const extraArgs = '--sandbox read-only --ask-for-approval on-request --model "o3 mini"'
    const codex = launchCommand('codex', { ...auto, program: 'codex', extraArgs })
    expect(codex.indexOf(extraArgs)).toBeGreaterThan(
      codex.indexOf('--sandbox workspace-write --ask-for-approval never'),
    )
    expect(codex.indexOf(extraArgs)).toBeGreaterThan(codex.lastIndexOf(' -c '))
    expect(codex.endsWith(` ${extraArgs} 'Fix it'`)).toBe(true)
  })

  it('adds the words of NATAMUX_CODEX_ARGS without mode flags in the ask mode', () => {
    const codex = launchCommand('codex', { ...base, program: 'codex', extraArgs: '--model o3' })
    expect(codex.endsWith(" --model o3 'Fix it'")).toBe(true)
    expect(codex).not.toContain('--sandbox')
  })

  it('sets the Gemini settings in PowerShell on Windows, and leaves the prompt out when empty', () => {
    expect(
      launchCommand('gemini', { ...base, program: 'gemini', platform: 'win32', prompt: '' }),
    ).toBe("$env:GEMINI_CLI_SYSTEM_SETTINGS_PATH='/data/gemini.json'; gemini")
  })
})

describe('oneShotCommand', () => {
  it.each([
    ['claude-code', 'claude -p', 'Write it'],
    ['codex', 'codex exec -', 'Write it'],
    ['gemini', "gemini -p 'Answer the request above.'", 'Write it'],
    ['copilot', "copilot -p 'Write it'", ''],
    ['cursor', "cursor-agent -p --output-format text 'Write it'", ''],
  ] as const)('runs %s once, with the prompt on stdin or as a word', (agent, line, stdin) => {
    expect(oneShotCommand(agent, DEFAULT_PROGRAMS[agent], 'Write it', 'linux')).toEqual({
      line,
      stdin,
    })
  })

  it.each(LAUNCHABLE_AGENTS)('starts %s with no permission flags, hooks or settings', (agent) => {
    const { line } = oneShotCommand(agent, DEFAULT_PROGRAMS[agent], 'Write it', 'linux')
    for (const flag of [...AUTO_FLAGS[agent], '--settings', 'natamux hook', '-i']) {
      expect(line.split(' '), flag).not.toContain(flag)
    }
    expect(line).not.toContain('GEMINI_CLI_SYSTEM_SETTINGS_PATH')
  })

  it.skipIf(process.platform === 'win32')(
    'gives a prompt with quotes, $, backticks, newlines, ; and && as one word, unchanged',
    () => {
      const prompt = `it's "$HOME" \`id\` $(id)\nnext; echo x && echo y`
      const { line } = oneShotCommand('copilot', 'printf %s', prompt, 'linux')
      const shell = ['-c', line.replace(' -p ', ' ')]
      expect(execFileSync('/bin/sh', shell, { encoding: 'utf8' })).toBe(prompt)
    },
  )

  it('keeps the words of a configured program and quotes the prompt for the shell', () => {
    expect(oneShotCommand('copilot', 'node /opt/copilot.js', "it's", 'linux').line).toBe(
      "node /opt/copilot.js -p 'it'\\''s'",
    )
    expect(oneShotCommand('cursor', 'agent', "it's", 'win32').line).toBe(
      "agent -p --output-format text 'it''s'",
    )
  })
})

describe('agentOfCommand', () => {
  const defaults = (agent: LaunchableAgent) => DEFAULT_PROGRAMS[agent]

  it.each([
    ['claude --settings /data/claude.json Fix it', 'claude-code'],
    ['/Users/me/.local/bin/claude', 'claude-code'],
    ['codex --sandbox workspace-write', 'codex'],
    // Installed with npm: the program is Node.js, the agent is its script.
    ['node /usr/local/bin/codex', 'codex'],
    ['/usr/local/bin/node --no-warnings /opt/lib/node_modules/@openai/codex/bin/codex.js', 'codex'],
    ['node /opt/homebrew/bin/gemini -i Fix it', 'gemini'],
    ['node /opt/homebrew/bin/copilot', 'copilot'],
    ['/bin/bash /Users/me/.local/bin/cursor-agent --force', 'cursor'],
    ['codex --model x', 'codex'],
    ['claude -p hi', 'claude-code'],
    ['/usr/local/bin/codex', 'codex'],
    ['cursor-agent', 'cursor'],
    // Cursor's own launcher keeps the name it was called by, `agent`, in front of Node.js.
    [
      '/Users/me/.local/bin/agent --use-system-ca /Users/me/.local/share/cursor-agent/versions/1/index.js',
      'cursor',
    ],
    ['agent', 'cursor'],
  ])('finds the agent of %s', (line, agent) => {
    expect(agentOfCommand(line, defaults)).toBe(agent)
  })

  it.each([
    'vim codex.md',
    'node server.js',
    'bash ./build.sh',
    'sleep 60',
    '',
    // Only the name of an agent counts, not a name that holds one.
    'claude-foo',
    'my-codex',
    // Node.js itself, or a tool that runs on it.
    'node',
    'node /usr/local/bin/pnpm dev',
    'node /usr/local/bin/npm run dev',
    'node /home/u/codex-notes/server.js',
    'bash',
    'htop',
    'less codex.log',
    // An agent on another machine is not in this shell.
    'ssh dev codex',
    'docker exec -it box claude',
  ])('finds no agent in %s', (line) => {
    expect(agentOfCommand(line, defaults)).toBeUndefined()
  })

  it('knows an agent by the program configured for it', () => {
    const programs = { ...DEFAULT_PROGRAMS, 'claude-code': '/opt/tools/my-claude --verbose' }
    const programOf = (agent: LaunchableAgent) => programs[agent]
    expect(agentOfCommand('/opt/tools/my-claude --verbose', programOf)).toBe('claude-code')
    expect(agentOfCommand('node /opt/tools/my-claude', programOf)).toBe('claude-code')
    // The default name still counts, for an agent the user starts by hand.
    expect(agentOfCommand('claude', programOf)).toBe('claude-code')
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

  it('writes the hook files of all agents, for an agent started by hand in a shell', () => {
    prepareAgentHooks(repo)
    expect(readFileSync(join(repo, '.cursor/hooks.json'), 'utf8')).toContain('natamux hook cursor')
    expect(existsSync(join(repo, '.github/hooks/natamux.json'))).toBe(true)
    expect(execFileSync('git', ['status', '--porcelain'], { cwd: repo, encoding: 'utf8' })).toBe('')
  })

  it('needs no file for agents that take their hooks another way', () => {
    expect(prepareWorkspace('codex', repo)).toBe(true)
    expect(existsSync(join(repo, '.codex'))).toBe(false)
  })
})
