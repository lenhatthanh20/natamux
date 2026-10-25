import { execFileSync } from 'node:child_process'
import { appendFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { basename, dirname, isAbsolute, join, relative, sep } from 'node:path'
import { CLAUDE_CODE_HOOK_EVENTS, type PermissionMode, TASK_ID_HEADER } from '@natamux/core'
import { SHELLS } from '../terminals/terminal-limits.js'

// Agents that Natamux can start. Each one also has an adapter that reads its hooks.
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

// Other names an agent is installed under. Cursor's installer adds `agent` next to `cursor-agent`.
const OTHER_PROGRAMS: Partial<Record<LaunchableAgent, string[]>> = { cursor: ['agent'] }

// Programs that run the script of an agent: one installed with npm runs as `node <path>/codex`.
const RUNTIMES = ['node', 'bun', 'deno', ...SHELLS]

// `/usr/lib/node_modules/@openai/codex/bin/codex.js` names the program `codex`.
const programName = (word: string) => basename(word).replace(/\.[cm]?js$|\.exe$/, '')

// The agent a command line runs, by its program: the one configured for an agent or the default.
// After a runtime, the script counts.
export function agentOfCommand(
  line: string,
  programOf: (agent: LaunchableAgent) => string,
): LaunchableAgent | undefined {
  const words = line.trim().split(/\s+/)
  const first = programName(words[0] ?? '')
  const script = words.slice(1).find((word) => !word.startsWith('-'))
  const name = RUNTIMES.includes(first) && script !== undefined ? programName(script) : first
  return LAUNCHABLE_AGENTS.find((agent) =>
    [
      DEFAULT_PROGRAMS[agent],
      ...(OTHER_PROGRAMS[agent] ?? []),
      programOf(agent).trim().split(/\s+/)[0] ?? '',
    ]
      .map(programName)
      .includes(name),
  )
}

// The flags of the `auto` permission mode. Only Claude Code checks each action in its auto mode;
// the others have no such check, so their closest flag lets them run (almost) anything.
export const AUTO_FLAGS: Record<LaunchableAgent, string[]> = {
  'claude-code': ['--permission-mode', 'auto'],
  // Codex dropped `--full-auto`. These two flags mean the same and exist in old and new versions.
  codex: ['--sandbox', 'workspace-write', '--ask-for-approval', 'never'],
  gemini: ['--approval-mode', 'yolo'],
  copilot: ['--allow-all-tools'],
  cursor: ['--force'],
}

// What the hooks run. `natamux hook` does nothing outside a Natamux terminal (no token).
const HOOK = 'natamux hook'

// Quotes one argument for the shell of the terminal: PowerShell on Windows, a POSIX shell elsewhere.
export function quoteArgument(value: string, platform: NodeJS.Platform = process.platform): string {
  if (platform === 'win32') {
    // PowerShell also ends a quoted string at the typographic single quotes, which titles of
    // imported issues often hold. Doubling each one keeps it inside the string.
    return `'${value.replace(/['\u2018\u2019\u201A\u201B]/g, '$&$&')}'`
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

// The TOML value of one event's hooks.
function codexHookValue(event: (typeof CODEX_EVENTS)[number]): string {
  const tools: readonly string[] = ['PreToolUse', 'PostToolUse', 'PermissionRequest']
  return `[{${tools.includes(event) ? 'matcher="*",' : ''}hooks=[{type="command",command="${HOOK} codex",timeout=5}]}]`
}

export function codexHookArgs(): string[] {
  return CODEX_EVENTS.flatMap((event) => ['-c', `hooks.${event}=${codexHookValue(event)}`])
}

// The first line of the Codex project config Natamux writes; a file without it is the user's.
const CODEX_FILE_MARK = '# Natamux hooks: Natamux writes this file again.'

// The same hooks as codexHookArgs, for a Codex started by hand. Codex reads it only in a folder
// it trusts. A Codex that Natamux starts gets them again with `-c`, which replaces each key, so
// every hook runs once.
export function codexHookFile(): string {
  const lines = CODEX_EVENTS.map((event) => `${event} = ${codexHookValue(event)}`)
  return `${[CODEX_FILE_MARK, '[hooks]', ...lines].join('\n')}\n`
}

// Gemini CLI reads these as its system settings when GEMINI_CLI_SYSTEM_SETTINGS_PATH names the
// file, so the user's own settings stay as they are. Its timeouts are in milliseconds.
export function geminiHookSettings() {
  const hook = { type: 'command', name: 'natamux', command: `${HOOK} gemini`, timeout: 5000 }
  const events = ['BeforeAgent', 'BeforeTool', 'AfterTool', 'Notification', 'AfterAgent']
  return {
    // Inline, so its output goes into the terminal history. System settings win over the user's.
    ui: { useAlternateBuffer: false },
    hooks: Object.fromEntries(
      events.map((event) => [
        event,
        [event.endsWith('Tool') ? { matcher: '.*', hooks: [hook] } : { hooks: [hook] }],
      ]),
    ),
  }
}

// Claude Code's hooks for a Claude Code started by hand. Commands, not the `http` hooks of
// `--settings`: Copilot CLI reads this file too and refuses an `http` hook on loopback.
// `natamux hook claude-code` does nothing when Copilot runs it.
export function claudeCodeHookFile() {
  const hook = { type: 'command', command: `${HOOK} claude-code`, timeout: 5 }
  return {
    hooks: Object.fromEntries(CLAUDE_CODE_HOOK_EVENTS.map((event) => [event, [{ hooks: [hook] }]])),
  }
}

// Copilot CLI does not tell a hook which event it is, so each event names itself.
const COPILOT_EVENTS = [
  'userPromptSubmitted',
  'preToolUse',
  'postToolUse',
  'postToolUseFailure',
  'notification',
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

// A file an agent reads from the workspace, so an agent the user starts by hand reports too.
export type WorkspaceFile = { path: string; content: string }

// Whether to write Claude Code's file. With it in place a Claude Code that Natamux starts gets no
// `--settings`, so only the daemon turns it on.
export type WorkspaceHooks = { claudeCode?: boolean }

const json = (value: unknown) => `${JSON.stringify(value, null, 2)}\n`

type HookFileOf = (hooks: WorkspaceHooks) => WorkspaceFile | undefined

const WORKSPACE_FILES: Record<LaunchableAgent, HookFileOf> = {
  // The launch of Claude Code and of Gemini leaves out its own copy when this file is in place
  // (see launchCommand).
  'claude-code': ({ claudeCode }) =>
    claudeCode === true
      ? { path: '.claude/settings.local.json', content: json(claudeCodeHookFile()) }
      : undefined,
  gemini: () => ({ path: '.gemini/settings.json', content: json(geminiHookSettings()) }),
  codex: () => ({ path: '.codex/config.toml', content: codexHookFile() }),
  copilot: () => ({ path: '.github/hooks/natamux.json', content: json(copilotHookFile()) }),
  cursor: () => ({ path: '.cursor/hooks.json', content: json(cursorHookFile()) }),
}

// Codex splits a `-c` key at each dot, so the path goes into a table value. That may hide the
// user's other `projects` entries for this run, which needs only this one. A JSON string is a
// valid TOML string.
function codexTrustArg(workspace: string): string {
  return `projects={${JSON.stringify(workspace)}={trust_level="trusted"}}`
}

// In place of a session id: the newest Claude Code session of the folder. No id looks like it.
export const LAST_SESSION = '--continue'

export type LaunchOptions = {
  program: string
  // Words added to the command line, from NATAMUX_<AGENT>_ARGS.
  extraArgs: string
  prompt: string
  // A session of the agent to go on with, or LAST_SESSION.
  session?: string
  platform: NodeJS.Platform
  claudeSettingsPath: string
  geminiSettingsPath: string
  permissionMode: PermissionMode
  // The task's worktree, which the agent is told to trust. Null: no trust is added.
  workspace: string | null
  // The agent's hook file is in the worktree (prepareWorkspace), so the launch need not pass them.
  workspaceHooks?: boolean
}

// The command line that starts an agent in its interactive mode, with its first prompt and the
// hooks that report to the daemon. Each stays open for the prompts of the next stages.
export function launchCommand(agent: LaunchableAgent, options: LaunchOptions): string {
  const quote = (value: string) => quoteArgument(value, options.platform)
  // The mode comes before the extra words, so words of the user's own can still change it.
  const mode = options.permissionMode === 'auto' ? AUTO_FLAGS[agent] : []
  const extra = options.extraArgs === '' ? [] : [options.extraArgs]
  const prompt = options.prompt === '' ? [] : [quote(options.prompt)]
  const session = options.session
  // `--resume <id>`, or the flag of the newest session of the folder.
  const resume = (newest: string[]) =>
    session === undefined ? [] : session === LAST_SESSION ? newest : ['--resume', quote(session)]
  switch (agent) {
    case 'claude-code':
      return [
        options.program,
        // Only without the workspace file: with both, Claude Code runs each hook twice.
        ...(options.workspaceHooks === true
          ? []
          : ['--settings', quote(options.claudeSettingsPath)]),
        ...mode,
        ...resume([LAST_SESSION]),
        ...extra,
        ...prompt,
      ].join(' ')
    case 'codex':
      return [
        options.program,
        // `codex resume [OPTIONS] [SESSION_ID]` takes the same options as `codex`.
        ...(session === undefined ? [] : ['resume']),
        '--dangerously-bypass-hook-trust',
        ...mode,
        ...(options.workspace === null ? [] : ['-c', quote(codexTrustArg(options.workspace))]),
        ...codexHookArgs().map((arg) => (arg === '-c' ? arg : quote(arg))),
        // Inline, so its output goes into the terminal history. Codex exits when the flag
        // comes twice, so the user's own one is enough.
        ...(options.extraArgs.split(/\s+/).includes('--no-alt-screen') ? [] : ['--no-alt-screen']),
        ...extra,
        ...(session === undefined ? [] : session === LAST_SESSION ? ['--last'] : [quote(session)]),
        ...prompt,
      ].join(' ')
    case 'gemini': {
      const command = [
        options.program,
        ...mode,
        ...extra,
        // A bare `--resume` is the newest session.
        ...resume(['--resume']),
        ...(prompt.length > 0 ? ['-i', ...prompt] : []),
      ]
      const settings = quote(options.geminiSettingsPath)
      // Folder trust is off by default; when the user turned it on, this skips its question. With
      // the hooks in the workspace's settings, the system settings would run each hook twice.
      const system = options.workspaceHooks !== true
      if (options.platform === 'win32') {
        const env = system ? ` $env:GEMINI_CLI_SYSTEM_SETTINGS_PATH=${settings};` : ''
        return `$env:GEMINI_CLI_TRUST_WORKSPACE='true';${env} ${command.join(' ')}`
      }
      const env = system ? ` GEMINI_CLI_SYSTEM_SETTINGS_PATH=${settings}` : ''
      return `GEMINI_CLI_TRUST_WORKSPACE=true${env} ${command.join(' ')}`
    }
    case 'copilot':
      return [
        options.program,
        ...mode,
        ...extra,
        ...resume(['--continue']),
        ...(prompt.length > 0 ? ['-i', ...prompt] : []),
      ].join(' ')
    case 'cursor':
      return [options.program, ...mode, ...extra, ...resume(['--continue']), ...prompt].join(' ')
  }
}

// The command line that runs an agent once, without its interactive screen, for a short answer
// such as a commit message: the prompt goes on stdin where the agent reads it there, so a long
// prompt does not hit the limit of a command line. No hooks, no permission flags: it needs no
// tools, and must not move the task.
export function oneShotCommand(
  agent: LaunchableAgent,
  program: string,
  prompt: string,
  platform: NodeJS.Platform = process.platform,
): { line: string; stdin: string } {
  const quote = (value: string) => quoteArgument(value, platform)
  switch (agent) {
    case 'claude-code':
      return { line: `${program} -p`, stdin: prompt }
    case 'codex':
      return { line: `${program} exec -`, stdin: prompt }
    case 'gemini':
      // Gemini puts the text of -p after what it reads on stdin.
      return { line: `${program} -p ${quote('Answer the request above.')}`, stdin: prompt }
    case 'copilot':
      return { line: `${program} -p ${quote(prompt)}`, stdin: '' }
    case 'cursor':
      return { line: `${program} -p --output-format text ${quote(prompt)}`, stdin: '' }
  }
}

// One hook handler of Natamux's: a command that runs `natamux hook`, or Claude Code's http hook,
// which names the task in a header. Cursor names a command `command`; Copilot `bash` and
// `powershell`.
function isNatamuxHook(hook: unknown): boolean {
  if (typeof hook !== 'object' || hook === null) {
    return false
  }
  const { command, bash, powershell, url, headers } = hook as Record<string, unknown>
  if (url !== undefined) {
    return (
      typeof url === 'string' &&
      typeof headers === 'object' &&
      headers !== null &&
      TASK_ID_HEADER in headers
    )
  }
  const commands = [command, bash, powershell].filter((each) => each !== undefined)
  return (
    commands.length > 0 &&
    commands.every((each) => typeof each === 'string' && each.startsWith(HOOK))
  )
}

// The JSON of a hook file that holds Natamux's hooks only, so writing them again loses no hook of
// the user's. Other keys (Claude Code keeps the permissions the user allowed here) stay. Undefined
// for any other file. Claude Code and Gemini put the handlers of an event in `hooks`.
function natamuxHookJson(text: string): Record<string, unknown> | undefined {
  let parsed: unknown
  try {
    parsed = JSON.parse(text)
  } catch {
    return undefined
  }
  const hooks = (parsed as { hooks?: unknown } | null)?.hooks
  if (typeof hooks !== 'object' || hooks === null || Array.isArray(hooks)) {
    return undefined
  }
  const handlers = Object.values(hooks)
    .flat()
    .flatMap((entry) => (Array.isArray(entry?.hooks) ? entry.hooks : [entry]))
  return handlers.length > 0 && handlers.every(isNatamuxHook)
    ? (parsed as Record<string, unknown>)
    : undefined
}

// The text to write over `current`, or undefined to leave a file of the user's alone.
function hookFileText(file: WorkspaceFile, current: string): string | undefined {
  if (current === file.content) {
    return current
  }
  if (file.path.endsWith('.toml')) {
    return current.startsWith(CODEX_FILE_MARK) ? file.content : undefined
  }
  const old = natamuxHookJson(current)
  return old === undefined ? undefined : json({ ...old, ...JSON.parse(file.content) })
}

// Writes the agent's hook file into the workspace and keeps it out of git. A file with a hook of
// the user's own at that place is left alone: the agent then runs without Natamux's hooks, and its
// terminal still shows that it works. Returns whether Natamux's hooks are in place.
export function prepareWorkspace(
  agent: LaunchableAgent,
  workspace: string,
  hooks: WorkspaceHooks = {},
): boolean {
  const file = WORKSPACE_FILES[agent](hooks)
  if (file === undefined) {
    return false
  }
  const target = join(workspace, file.path)
  const text = existsSync(target) ? hookFileText(file, readFileSync(target, 'utf8')) : file.content
  if (text === undefined) {
    return false
  }
  try {
    mkdirSync(dirname(target), { recursive: true })
    writeFileSync(target, text)
  } catch {
    // A folder that cannot be written: the agent still starts, followed by its terminal only.
    return false
  }
  excludeFromGit(workspace, `/${file.path}`)
  return true
}

// The hook files of all agents, so an agent the user starts by hand in any shell reports to the
// daemon too.
export function prepareAgentHooks(workspace: string, hooks: WorkspaceHooks = {}): void {
  for (const agent of LAUNCHABLE_AGENTS) {
    prepareWorkspace(agent, workspace, hooks)
  }
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

// Where Cursor and Copilot keep the folders the user trusted.
export type TrustPaths = { cursorProjects: string; copilotConfig: string }

export function trustPaths(env: NodeJS.ProcessEnv, home: string): TrustPaths {
  return {
    cursorProjects: join(env.CURSOR_DATA_DIR?.trim() || join(home, '.cursor'), 'projects'),
    copilotConfig: join(env.COPILOT_HOME?.trim() || join(home, '.copilot'), 'config.json'),
  }
}

// Marks the worktree as trusted, so Cursor and Copilot do not ask at start. Neither has a flag
// for it in interactive mode. Codex and Gemini get theirs on the command line. Best effort: when
// it fails, the agent asks as before.
export function trustWorkspace(agent: LaunchableAgent, workspace: string, paths: TrustPaths): void {
  try {
    if (agent === 'cursor') {
      // The folder name and marker file are those cursor-agent writes when the user trusts it.
      const slug = workspace
        .replace(/[^a-zA-Z0-9]/g, '-')
        .replace(/-+/g, '-')
        .replace(/^-+|-+$/g, '')
      const dir = join(paths.cursorProjects, slug)
      mkdirSync(dir, { recursive: true })
      const marker = { trustedAt: new Date().toISOString(), workspacePath: workspace }
      writeFileSync(join(dir, '.workspace-trusted'), JSON.stringify(marker, null, 2), {
        flag: 'wx',
      })
    } else if (agent === 'copilot') {
      trustInCopilot(workspace, paths.copilotConfig)
    }
  } catch {
    // Already trusted, or a file that cannot be written.
  }
}

// Copilot trusts a listed folder and everything below it. Its config starts with comment lines,
// kept as they are. A missing or broken config is left alone.
function trustInCopilot(workspace: string, configPath: string): void {
  const text = readFileSync(configPath, 'utf8')
  const header = text.match(/^(\s*\/\/[^\n]*\n)*/)?.[0] ?? ''
  const config = JSON.parse(text.slice(header.length))
  const folders: unknown = config.trustedFolders ?? []
  if (!Array.isArray(folders)) {
    return
  }
  const covers = (folder: unknown) => {
    if (typeof folder !== 'string') {
      return false
    }
    const path = relative(folder, workspace)
    return path.split(sep)[0] !== '..' && !isAbsolute(path)
  }
  if (folders.some(covers)) {
    return
  }
  config.trustedFolders = [...folders, workspace]
  writeFileSync(configPath, `${header}${JSON.stringify(config, null, 2)}\n`)
}
