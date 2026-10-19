import { mkdirSync, writeFileSync } from 'node:fs'
import { homedir } from 'node:os'
import { dirname, join } from 'node:path'
import { plainText, type Task, waitingOn } from '@natamux/core'
import type { TaskService } from '../tasks/task-service.js'
import { MAIN_TERMINAL } from '../terminals/terminal-limits.js'
import type { TerminalService } from '../terminals/terminal-service.js'
import { UnknownAgentError } from './agent-hook-service.js'
import {
  DEFAULT_PROGRAMS,
  isLaunchable,
  LAST_SESSION,
  type LaunchableAgent,
  launchCommand,
  prepareWorkspace,
  quoteArgument,
  type TrustPaths,
  trustPaths,
  trustWorkspace,
  type WorkspaceHooks,
} from './agent-launch.js'

export class TaskWaitingError extends Error {
  constructor(taskId: string, waiting: readonly string[]) {
    super(`${taskId} waits for ${waiting.join(', ')} to be done`)
    this.name = 'TaskWaitingError'
  }
}

export { LAUNCHABLE_AGENTS, quoteArgument } from './agent-launch.js'

// On Windows the program of a terminal is always the shell, so the end of an agent cannot be seen
// there. Its launch line sets these window titles, which reach the terminal as escape sequences:
// the first as the agent starts, the second once it has ended (see StageRunner.onOutput).
export const AGENT_TITLE = 'natamux-agent'
export const AGENT_EXITED_TITLE = 'natamux-agent-exited'

// The first prompt: the task title, then its description. On one line, because the shell runs
// the command at the first line end.
export function firstPrompt(task: Pick<Task, 'title' | 'description'>): string {
  return [task.title, task.description]
    .map((part) => part.replace(/\s+/g, ' ').trim())
    .filter((part) => part !== '')
    .join(' - ')
}

type AgentRunnerOptions = {
  tasks: TaskService
  terminals: TerminalService
  // Claude Code settings with the hooks that report to this daemon (see claudeCodeHookSettings).
  claudeSettingsPath: string
  // Gemini CLI settings with its hooks (see geminiHookSettings). Default: next to Claude's.
  geminiSettingsPath?: string
  // What the hook files in the workspace need from the daemon (see prepareWorkspace).
  workspaceHooks?: WorkspaceHooks
  // The command of each agent (NATAMUX_<AGENT>_COMMAND), and words to add to its command line
  // (NATAMUX_<AGENT>_ARGS), such as --model sonnet for Claude Code.
  programs?: Partial<Record<LaunchableAgent, string | undefined>>
  extraArgs?: Partial<Record<LaunchableAgent, string | undefined>>
  claudeCommand?: string
  copilotCommand?: string
  copilotArgs?: string
  platform?: NodeJS.Platform
  // Where the launch scripts go. Default: the folder of the settings file.
  launchDir?: string
  // The folder of the app's `natamux` command (NATAMUX_BIN_DIR), put first on the agent's PATH.
  binDir?: string
  // Told of each start, to watch the terminal for a shell that cannot find the agent.
  onLaunch?: (launch: AgentLaunch) => void
  // Where the agents keep their trusted folders. Default: from the user's home and environment.
  trustPaths?: TrustPaths
}

export type AgentLaunch = {
  taskId: string
  terminal: string
  agent: LaunchableAgent
  program: string
  // The session the agent went on with.
  session?: string
}

// Starts an agent in the terminal of a task, as if the user typed the command. Its hooks then
// move the task from state to state.
export class AgentRunner {
  private readonly tasks: TaskService
  private readonly terminals: TerminalService
  private readonly claudeSettingsPath: string
  private readonly geminiSettingsPath: string
  private readonly workspaceHooks: WorkspaceHooks
  private readonly programs: Record<LaunchableAgent, string>
  private readonly extraArgs: Partial<Record<LaunchableAgent, string>>
  private readonly platform: NodeJS.Platform
  private readonly launchDir: string
  private readonly binDir: string | undefined
  private readonly onLaunch: ((launch: AgentLaunch) => void) | undefined
  private readonly trustPaths: TrustPaths

  constructor(options: AgentRunnerOptions) {
    this.tasks = options.tasks
    this.terminals = options.terminals
    this.claudeSettingsPath = options.claudeSettingsPath
    this.geminiSettingsPath =
      options.geminiSettingsPath ??
      join(dirname(options.claudeSettingsPath), 'gemini-settings.json')
    this.workspaceHooks = options.workspaceHooks ?? {}
    const given = { ...options.programs }
    given['claude-code'] ??= options.claudeCommand
    given.copilot ??= options.copilotCommand
    this.programs = { ...DEFAULT_PROGRAMS }
    for (const [agent, program] of Object.entries(given)) {
      if (program?.trim() && isLaunchable(agent)) {
        this.programs[agent] = program.trim()
      }
    }
    this.extraArgs = { copilot: options.copilotArgs?.trim() }
    for (const [agent, words] of Object.entries(options.extraArgs ?? {})) {
      if (words !== undefined && isLaunchable(agent)) {
        this.extraArgs[agent] = words.trim()
      }
    }
    this.platform = options.platform ?? process.platform
    this.launchDir = options.launchDir ?? dirname(options.claudeSettingsPath)
    this.binDir = options.binDir?.trim() || undefined
    this.onLaunch = options.onLaunch
    this.trustPaths = options.trustPaths ?? trustPaths(process.env, homedir())
  }

  // The program that starts an agent, as the shell looks it up.
  programOf(agent: LaunchableAgent): string {
    return this.programs[agent]
  }

  // Starts the agent in a terminal of the task: the first by default, another for a stage that
  // runs beside the first one. With a session, the agent goes on with it and gets no prompt.
  // A stage with its own agent or words names itself, so the task keeps its agent. So does an
  // agent `restored` in its shell after a restart.
  start(
    taskId: string,
    agent: string,
    prompt?: string,
    terminal?: string,
    session?: string,
    own: { args?: string; stage?: string; restored?: boolean } = {},
  ): Task {
    if (!isLaunchable(agent)) {
      throw new UnknownAgentError(agent)
    }
    const task = this.tasks.getTask(taskId)
    const waiting = waitingOn(task, this.tasks.listTasks())
    if (waiting.length > 0) {
      throw new TaskWaitingError(taskId, waiting)
    }
    let workspaceHooks = false
    if (task.workspace !== null) {
      workspaceHooks = prepareWorkspace(agent, task.workspace, this.workspaceHooks)
      trustWorkspace(agent, task.workspace, this.trustPaths)
    }
    const command = launchCommand(agent, {
      program: this.programs[agent],
      extraArgs: [this.extraArgs[agent], own.args].filter(Boolean).join(' '),
      prompt:
        session === undefined
          ? plainText(prompt ?? firstPrompt(task))
              .replace(/\s+/g, ' ')
              .trim()
          : '',
      session,
      platform: this.platform,
      claudeSettingsPath: this.claudeSettingsPath,
      geminiSettingsPath: this.geminiSettingsPath,
      // Read at each start, so a saved change counts for the next agent.
      permissionMode: this.tasks.settingsOf(task.repo).agents.permissionMode,
      workspace: task.workspace,
      workspaceHooks,
    })

    this.terminals.run(taskId, this.launchLine(taskId, command, terminal), terminal)
    const program = this.programs[agent]
    const shell = terminal ?? MAIN_TERMINAL
    const resumed = session === undefined || session === LAST_SESSION ? {} : { session }
    this.onLaunch?.({ taskId, terminal: shell, agent, program, ...resumed })
    const restoredIn = own.restored === true ? shell : undefined
    return this.tasks.recordAgentLaunch(taskId, agent, program, own.stage, restoredIn)
  }

  // What is typed into the terminal. The terminal echoes it, so the long command (settings path,
  // prompt) goes into a small script and only a short line to run it is typed. The script is
  // sourced, not run by `sh`, so the user's own shell (aliases, PATH) still starts the agent.
  // PowerShell may refuse scripts by policy, so Windows types the command itself, between the
  // window titles that tell when the agent starts and ends.
  // A login shell's profile (path_helper, brew shellenv) can push the app's bin folder behind an
  // older global `natamux`. The PATH prefix puts it first again, for the agent and its hooks only.
  private launchLine(taskId: string, command: string, terminal?: string): string {
    if (this.platform === 'win32') {
      const title = (text: string) => `$Host.UI.RawUI.WindowTitle = '${text}'`
      return `${title(AGENT_TITLE)}; ${command}; ${title(AGENT_EXITED_TITLE)}`
    }
    mkdirSync(this.launchDir, { recursive: true })
    const suffix = terminal === undefined || terminal === '1' ? '' : `-${terminal}`
    const script = join(this.launchDir, `agent-launch-${taskId}${suffix}.sh`)
    const path =
      this.binDir === undefined ? '' : `PATH=${quoteArgument(this.binDir, this.platform)}:"$PATH" `
    writeFileSync(script, `${path}${command}\n`, { mode: 0o600 })
    return `. ${quoteArgument(script, this.platform)}`
  }
}
