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
export const LAUNCHABLE_AGENTS = ['claude-code'] as const

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
  platform?: NodeJS.Platform
}

// Starts an agent in the terminal of a task, as if the user typed the command. Its hooks then
// move the task from state to state.
export class AgentRunner {
  private readonly tasks: TaskService
  private readonly terminals: TerminalService
  private readonly claudeSettingsPath: string
  private readonly claudeCommand: string
  private readonly platform: NodeJS.Platform

  constructor(options: AgentRunnerOptions) {
    this.tasks = options.tasks
    this.terminals = options.terminals
    this.claudeSettingsPath = options.claudeSettingsPath
    this.claudeCommand = options.claudeCommand?.trim() || 'claude'
    this.platform = options.platform ?? process.platform
  }

  start(taskId: string, agent: string, prompt?: string): Task {
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
    const line = [
      this.claudeCommand,
      '--settings',
      quote(this.claudeSettingsPath),
      ...(text === '' ? [] : [quote(text)]),
    ].join(' ')

    this.terminals.run(taskId, line)
    return this.tasks.recordAgentLaunch(taskId, agent)
  }
}
