import type { TaskService } from '../tasks/task-service.js'
import { MAIN_TERMINAL } from '../terminals/terminal-limits.js'
import type { AgentLaunch } from './agent-runner.js'

// Agents whose hooks tell when they wait for the user. Copilot's hooks may be off until its folder is
// trusted, and Cursor's CLI has no such hook, so the bell still counts for them.
const AGENTS_WITH_HOOKS: readonly string[] = ['claude-code', 'codex', 'gemini']

const BELL = 0x07

// Output comes in many small pieces. Looking at the task for each would read its file each time.
const LOOK_EVERY_MS = 10_000
// Some agents ring the bell as they start (GitHub Copilot does); that is not a question.
export const START_BELL_MS = 15_000
// How long after a start the terminal is read for a shell that cannot find the agent.
export const LAUNCH_WATCH_MS = 15_000

// What shells say when they do not know a command: bash, zsh, sh and dash, fish, PowerShell, cmd.
const NOT_FOUND = /command not found|: not found|unknown command|is not recognized/i
// Colors and cursor moves would split the words of the line.
// biome-ignore lint/suspicious/noControlCharactersInRegex: escape sequences start with ESC
const ESCAPES = /\x1b\[[0-9;?]*[A-Za-z]|\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)/g

// The variable that sets an agent's command, as the daemon reads it.
function commandVariable(agent: string): string {
  return `NATAMUX_${agent === 'claude-code' ? 'CLAUDE' : agent.toUpperCase()}_COMMAND`
}

// The shell's line that says it could not find the program, if the text holds one.
export function notFoundLine(text: string, program: string): string | undefined {
  const name = program.split(/[\\/]/).at(-1) ?? program
  return text
    .replace(ESCAPES, '')
    .split(/\r?\n|\r/)
    .map((line) => line.trim())
    .find((line) => NOT_FOUND.test(line) && line.includes(name))
}

type LaunchWatch = AgentLaunch & { at: number; text: string }

// What any agent in a task's first terminal tells by its output alone:
// - output is a sign of life, so a running task does not look quiet while it prints;
// - the terminal bell, which many command line tools ring when they wait for the user, moves a
//   running task to needs input. Agents with hooks are left to their hooks.
export class GenericSignals {
  private readonly tasks: TaskService
  private readonly now: () => number
  private readonly lastLook = new Map<string, number>()
  private readonly launches = new Map<string, LaunchWatch>()

  constructor(tasks: TaskService, now: () => number = Date.now) {
    this.tasks = tasks
    this.now = now
  }

  // Reads the terminal of an agent that just started, for a shell that does not know it.
  watchLaunch(launch: AgentLaunch): void {
    this.launches.set(`${launch.taskId}/${launch.terminal}`, {
      ...launch,
      at: this.now(),
      text: '',
    })
  }

  onOutput(taskId: string, terminal: string, data: Buffer): void {
    this.checkLaunch(taskId, terminal, data)
    if (terminal !== MAIN_TERMINAL) {
      return
    }
    const bell = data.includes(BELL)
    const now = this.now()
    if (!bell && now - (this.lastLook.get(taskId) ?? 0) < LOOK_EVERY_MS) {
      return
    }
    this.lastLook.set(taskId, now)

    const task = this.tasks.getTask(taskId)
    if (task.state !== 'running') {
      return
    }
    const hasHooks = task.agent !== null && AGENTS_WITH_HOOKS.includes(task.agent)
    if (!hasHooks && bell) {
      const launched = this.tasks
        .listEvents(taskId)
        .filter((entry) => entry.event.type === 'agent.launched')
        .at(-1)
      if (launched !== undefined && now - Date.parse(launched.event.at) < START_BELL_MS) {
        return
      }
      this.tasks.applyEvent(taskId, {
        source: 'agent',
        type: 'agent.asked',
        // Inside a group, the terminal tells which stage asks.
        payload: { reason: 'terminal bell', terminal },
      })
      return
    }
    // Output also comes from a redraw or the user's typing, so only a hook ends a background wait.
    this.tasks.recordActivity(taskId, { endsWait: false })
  }

  // A shell that cannot find the agent prints so at once. The stage would wait for an agent that
  // never came, so the task fails with the shell's words and what to do.
  private checkLaunch(taskId: string, terminal: string, data: Buffer): void {
    const key = `${taskId}/${terminal}`
    const launch = this.launches.get(key)
    if (launch === undefined) {
      return
    }
    if (this.now() - launch.at > LAUNCH_WATCH_MS) {
      this.launches.delete(key)
      return
    }
    // A line can come in two pieces; the last bit of text is enough to join them.
    // Decoded only here, while a launch is watched: the rest needs no text.
    launch.text = (launch.text + data.toString()).slice(-4000)
    const line = notFoundLine(launch.text, launch.program)
    if (line === undefined) {
      return
    }
    this.launches.delete(key)
    this.tasks.applyEvent(taskId, {
      source: 'system',
      type: 'agent.failed',
      payload: {
        reason: `The agent did not start: "${line}". Install ${launch.program}, or set ${commandVariable(launch.agent)} to its command, then start the agent again.`,
        terminal,
      },
    })
  }
}
