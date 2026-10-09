import { mkdirSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { type Task, waitingOn } from '@helmsman/core'
import { UnknownAgentError } from './agent-hook-service.js'
import type { TaskService } from './task-service.js'
import type { TerminalService } from './terminal-service.js'

export class TaskWaitingError extends Error {
  constructor(taskId: string, waiting: readonly string[]) {
    super(`${taskId} waits for ${waiting.join(', ')} to be done`)
    this.name = 'TaskWaitingError'
  }
}

// Agents that Helmsman can start. Each one also has an adapter that reads its hooks.
export const LAUNCHABLE_AGENTS = ['claude-code', 'copilot'] as const

// Quotes one argument for the shell of the terminal: PowerShell on Windows, a POSIX shell elsewhere.
export function quoteArgument(value: string, platform: NodeJS.Platform = process.platform): string {
  if (platform === 'win32') {
    return `'${value.replaceAll("'", "''")}'`
  }
  return `'${value.replaceAll("'", "'\\''")}'`
}

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
  claudeCommand?: string
  // GitHub Copilot CLI, and words to add to its command line, such as --allow-all-tools.
  copilotCommand?: string
  copilotArgs?: string
  platform?: NodeJS.Platform
  // Where the launch scripts go. Default: the folder of the settings file.
  launchDir?: string
}

// Starts an agent in the terminal of a task, as if the user typed the command. Its hooks then
// move the task from state to state.
export class AgentRunner {
  private readonly tasks: TaskService
  private readonly terminals: TerminalService
  private readonly claudeSettingsPath: string
  private readonly claudeCommand: string
  private readonly copilotCommand: string
  private readonly copilotArgs: string
  private readonly platform: NodeJS.Platform
  private readonly launchDir: string

  constructor(options: AgentRunnerOptions) {
    this.tasks = options.tasks
    this.terminals = options.terminals
    this.claudeSettingsPath = options.claudeSettingsPath
    this.claudeCommand = options.claudeCommand?.trim() || 'claude'
    this.copilotCommand = options.copilotCommand?.trim() || 'copilot'
    this.copilotArgs = options.copilotArgs?.trim() ?? ''
    this.platform = options.platform ?? process.platform
    this.launchDir = options.launchDir ?? dirname(options.claudeSettingsPath)
  }

  // Starts the agent in a terminal of the task: the first by default, another for a stage that
  // runs beside the first one.
  start(taskId: string, agent: string, prompt?: string, terminal?: string): Task {
    if (!(LAUNCHABLE_AGENTS as readonly string[]).includes(agent)) {
      throw new UnknownAgentError(agent)
    }
    const task = this.tasks.getTask(taskId)
    const waiting = waitingOn(task, this.tasks.listTasks())
    if (waiting.length > 0) {
      throw new TaskWaitingError(taskId, waiting)
    }
    const text = (prompt ?? firstPrompt(task)).replace(/\s+/g, ' ').trim()
    const quote = (value: string) => quoteArgument(value, this.platform)
    // Copilot has no hooks for Helmsman yet: its output shows that it works, and it reports a
    // stage with `helmsman done`. `-i` keeps it open for the prompts of the next stages.
    const command =
      agent === 'copilot'
        ? [
            this.copilotCommand,
            ...(this.copilotArgs === '' ? [] : [this.copilotArgs]),
            ...(text === '' ? [] : ['-i', quote(text)]),
          ].join(' ')
        : [
            this.claudeCommand,
            '--settings',
            quote(this.claudeSettingsPath),
            ...(text === '' ? [] : [quote(text)]),
          ].join(' ')

    this.terminals.run(taskId, this.launchLine(taskId, command, terminal), terminal)
    return this.tasks.recordAgentLaunch(taskId, agent)
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
