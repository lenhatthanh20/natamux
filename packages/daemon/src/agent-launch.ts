import { execFileSync } from 'node:child_process'
import { appendFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { dirname, isAbsolute, join } from 'node:path'

// Agents that Helmsman can start. Each one also has an adapter that reads its hooks.
export const LAUNCHABLE_AGENTS = ['claude-code', 'codex', 'gemini', 'copilot', 'cursor'] as const

export type LaunchableAgent = (typeof LAUNCHABLE_AGENTS)[number]

export function isLaunchable(agent: string): agent is LaunchableAgent {
  return (LAUNCHABLE_AGENTS as readonly string[]).includes(agent)
}

// The command each agent is started with when no other is configured.
export const DEFAULT_PROGRAMS: Record<LaunchableAgent, string> = {
  'claude-code': 'claude',
  codex: 'codex',
  gemini: 'gemini',
  copilot: 'copilot',
  cursor: 'cursor-agent',
}

// What the hooks run. `helmsman hook` does nothing outside a Helmsman terminal (no token).
const HOOK = 'helmsman hook'

// Quotes one argument for the shell of the terminal: PowerShell on Windows, a POSIX shell elsewhere.
export function quoteArgument(value: string, platform: NodeJS.Platform = process.platform): string {
  if (platform === 'win32') {
    return `'${value.replaceAll("'", "''")}'`
  }
  return `'${value.replaceAll("'", "'\\''")}'`
}

// Codex takes hooks for one session as `-c` overrides. They count only with the trust check off
// for this run: Codex asks to trust every hook it has not seen, and these change with the daemon.
const CODEX_EVENTS = [
  'UserPromptSubmit',
  'PreToolUse',
  'PostToolUse',
  'PermissionRequest',
  'Stop',
  'Interrupt',
] as const

export function codexHookArgs(): string[] {
  const tools: readonly string[] = ['PreToolUse', 'PostToolUse', 'PermissionRequest']
  return CODEX_EVENTS.flatMap((event) => [
    '-c',
    `hooks.${event}=[{${tools.includes(event) ? 'matcher="*",' : ''}hooks=[{type="command",command="${HOOK} codex",timeout=5}]}]`,
  ])
}

// Gemini CLI reads these as its system settings when GEMINI_CLI_SYSTEM_SETTINGS_PATH names the
// file, so the user's own settings stay as they are. Its timeouts are in milliseconds.
export function geminiHookSettings() {
  const hook = { type: 'command', name: 'helmsman', command: `${HOOK} gemini`, timeout: 5000 }
  const events = ['BeforeAgent', 'BeforeTool', 'AfterTool', 'Notification', 'AfterAgent']
  return {
    hooks: Object.fromEntries(
      events.map((event) => [
        event,
        [event.endsWith('Tool') ? { matcher: '.*', hooks: [hook] } : { hooks: [hook] }],
      ]),
    ),
  }
}

// Copilot CLI does not tell a hook which event it is, so each event names itself.
const COPILOT_EVENTS = [
  'userPromptSubmitted',
  'preToolUse',
  'postToolUse',
  'postToolUseFailure',
  'permissionRequest',
  'agentStop',
  'errorOccurred',
] as const

export function copilotHookFile() {
  return {
    version: 1,
    hooks: Object.fromEntries(
      COPILOT_EVENTS.map((event) => {
        const command = `${HOOK} copilot ${event}`
        return [event, [{ type: 'command', bash: command, powershell: command, timeoutSec: 5 }]]
      }),
    ),
  }
}

// The Cursor CLI sends fewer events than the editor; the ones it does not send cost nothing.
const CURSOR_EVENTS = [
  'beforeSubmitPrompt',
  'preToolUse',
  'postToolUse',
  'postToolUseFailure',
  'beforeShellExecution',
  'afterShellExecution',
  'afterFileEdit',
  'stop',
] as const

export function cursorHookFile() {
  return {
    version: 1,
    hooks: Object.fromEntries(
      CURSOR_EVENTS.map((event) => [event, [{ command: `${HOOK} cursor`, timeout: 5 }]]),
    ),
  }
}

// A file an agent reads from the workspace, where no flag or variable can pass its hooks.
export type WorkspaceFile = { path: string; content: string }

const WORKSPACE_FILES: Partial<Record<LaunchableAgent, () => WorkspaceFile>> = {
  copilot: () => ({
    path: '.github/hooks/helmsman.json',
    content: `${JSON.stringify(copilotHookFile(), null, 2)}\n`,
  }),
  cursor: () => ({
    path: '.cursor/hooks.json',
    content: `${JSON.stringify(cursorHookFile(), null, 2)}\n`,
  }),
}

export type LaunchOptions = {
  program: string
  // Words added to the command line, from HELMSMAN_<AGENT>_ARGS.
  extraArgs: string
  prompt: string
  platform: NodeJS.Platform
  claudeSettingsPath: string
  geminiSettingsPath: string
}

// The command line that starts an agent in its interactive mode, with its first prompt and the
// hooks that report to the daemon. Each stays open for the prompts of the next stages.
export function launchCommand(agent: LaunchableAgent, options: LaunchOptions): string {
  const quote = (value: string) => quoteArgument(value, options.platform)
  const extra = options.extraArgs === '' ? [] : [options.extraArgs]
  const prompt = options.prompt === '' ? [] : [quote(options.prompt)]
  switch (agent) {
    case 'claude-code':
      return [
        options.program,
        '--settings',
        quote(options.claudeSettingsPath),
        ...extra,
        ...prompt,
      ].join(' ')
    case 'codex':
      return [
        options.program,
        '--dangerously-bypass-hook-trust',
        ...codexHookArgs().map((arg) => (arg === '-c' ? arg : quote(arg))),
        ...extra,
        ...prompt,
      ].join(' ')
    case 'gemini': {
      const command = [options.program, ...extra, ...(prompt.length > 0 ? ['-i', ...prompt] : [])]
      const settings = quote(options.geminiSettingsPath)
      return options.platform === 'win32'
        ? `$env:GEMINI_CLI_SYSTEM_SETTINGS_PATH=${settings}; ${command.join(' ')}`
        : `GEMINI_CLI_SYSTEM_SETTINGS_PATH=${settings} ${command.join(' ')}`
    }
    case 'copilot':
      return [options.program, ...extra, ...(prompt.length > 0 ? ['-i', ...prompt] : [])].join(' ')
    case 'cursor':
      return [options.program, ...extra, ...prompt].join(' ')
  }
}

// Writes the agent's hook file into the workspace and keeps it out of git. A file of the user's
// own at that place is left alone: the agent then runs without Helmsman's hooks, and its terminal
// still shows that it works. Returns whether Helmsman's hooks are in place.
export function prepareWorkspace(agent: LaunchableAgent, workspace: string): boolean {
  const file = WORKSPACE_FILES[agent]?.()
  if (file === undefined) {
    return true
  }
  const target = join(workspace, file.path)
  if (existsSync(target)) {
    const current = readFileSync(target, 'utf8')
    if (current !== file.content && !current.includes(HOOK)) {
      return false
    }
  }
  try {
    mkdirSync(dirname(target), { recursive: true })
    writeFileSync(target, file.content)
  } catch {
    // A folder that cannot be written: the agent still starts, followed by its terminal only.
    return false
  }
  excludeFromGit(workspace, `/${file.path}`)
  return true
}

// Adds a line to the repository's own ignore list, which all its worktrees share.
export function excludeFromGit(worktree: string, line: string): void {
  try {
    const common = execFileSync('git', ['rev-parse', '--git-common-dir'], {
      cwd: worktree,
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'ignore'],
    }).trim()
    const info = join(isAbsolute(common) ? common : join(worktree, common), 'info')
    const exclude = join(info, 'exclude')
    const text = existsSync(exclude) ? readFileSync(exclude, 'utf8') : ''
    if (text.split(/\r?\n/).includes(line)) {
      return
    }
    mkdirSync(info, { recursive: true })
    appendFileSync(exclude, `${text === '' || text.endsWith('\n') ? '' : '\n'}${line}\n`)
  } catch {
    // Not a git worktree: there is nothing to keep the file out of.
  }
}
