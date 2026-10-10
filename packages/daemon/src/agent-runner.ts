import { mkdirSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { type Task, waitingOn } from '@natamux/core'
import { UnknownAgentError } from './agent-hook-service.js'
import {
  DEFAULT_PROGRAMS,
  isLaunchable,
  type LaunchableAgent,
  launchCommand,
  prepareWorkspace,
  quoteArgument,
} from './agent-launch.js'
import type { TaskService } from './task-service.js'
import { MAIN_TERMINAL, type TerminalService } from './terminal-service.js'

export class TaskWaitingError extends Error {
  constructor(taskId: string, waiting: readonly string[]) {
    super(`${taskId} waits for ${waiting.join(', ')} to be done`)
    this.name = 'TaskWaitingError'
  }
}

export { LAUNCHABLE_AGENTS, quoteArgument } from './agent-launch.js'

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
  // The command of each agent (NATAMUX_<AGENT>_COMMAND), and words to add to its command line
  // (NATAMUX_<AGENT>_ARGS), such as --allow-all-tools for Copilot.
  programs?: Partial<Record<LaunchableAgent, string | undefined>>
  extraArgs?: Partial<Record<LaunchableAgent, string | undefined>>
  claudeCommand?: string
  copilotCommand?: string
  copilotArgs?: string
  platform?: NodeJS.Platform
  // Where the launch scripts go. Default: the folder of the settings file.
  launchDir?: string
  // Told of each start, to watch the terminal for a shell that cannot find the agent.
  onLaunch?: (launch: AgentLaunch) => void
}

export type AgentLaunch = {
  taskId: string
  terminal: string
  agent: LaunchableAgent
  program: string
}

// Starts an agent in the terminal of a task, as if the user typed the command. Its hooks then
// move the task from state to state.
export class AgentRunner {
  private readonly tasks: TaskService
  private readonly terminals: TerminalService
  private readonly claudeSettingsPath: string
  private readonly geminiSettingsPath: string
  private readonly programs: Record<LaunchableAgent, string>
  private readonly extraArgs: Partial<Record<LaunchableAgent, string>>
  private readonly platform: NodeJS.Platform
  private readonly launchDir: string
  private readonly onLaunch: ((launch: AgentLaunch) => void) | undefined

  constructor(options: AgentRunnerOptions) {
    this.tasks = options.tasks
    this.terminals = options.terminals
    this.claudeSettingsPath = options.claudeSettingsPath
    this.geminiSettingsPath =
      options.geminiSettingsPath ??
      join(dirname(options.claudeSettingsPath), 'gemini-settings.json')
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
    this.onLaunch = options.onLaunch
  }

  // The program that starts an agent, as the shell looks it up.
  programOf(agent: LaunchableAgent): string {
    return this.programs[agent]
  }

  // Starts the agent in a terminal of the task: the first by default, another for a stage that
  // runs beside the first one.
  start(taskId: string, agent: string, prompt?: string, terminal?: string): Task {
    if (!isLaunchable(agent)) {
      throw new UnknownAgentError(agent)
    }
    const task = this.tasks.getTask(taskId)
    const waiting = waitingOn(task, this.tasks.listTasks())
    if (waiting.length > 0) {
      throw new TaskWaitingError(taskId, waiting)
    }
    if (task.workspace !== null) {
      prepareWorkspace(agent, task.workspace)
    }
    const command = launchCommand(agent, {
      program: this.programs[agent],
      extraArgs: this.extraArgs[agent] ?? '',
      prompt: (prompt ?? firstPrompt(task)).replace(/\s+/g, ' ').trim(),
      platform: this.platform,
      claudeSettingsPath: this.claudeSettingsPath,
      geminiSettingsPath: this.geminiSettingsPath,
    })

    this.terminals.run(taskId, this.launchLine(taskId, command, terminal), terminal)
    const program = this.programs[agent]
    this.onLaunch?.({ taskId, terminal: terminal ?? MAIN_TERMINAL, agent, program })
    return this.tasks.recordAgentLaunch(taskId, agent, program)
  }

  // What is typed into the terminal. The terminal echoes it, so the long command (settings path,
  // prompt) goes into a small script and only a short line to run it is typed. The script is
  // sourced, not run by `sh`, so the user's own shell (aliases, PATH) still starts the agent.
  // PowerShell may refuse scripts by policy, so Windows types the command itself.
  private launchLine(taskId: string, command: string, terminal?: string): string {
    if (this.platform === 'win32') {
      return command
    }
    mkdirSync(this.launchDir, { recursive: true })
    const suffix = terminal === undefined || terminal === '1' ? '' : `-${terminal}`
    const script = join(this.launchDir, `agent-launch-${taskId}${suffix}.sh`)
    writeFileSync(script, `${command}\n`, { mode: 0o600 })
    return `. ${quoteArgument(script, this.platform)}`
  }
}
