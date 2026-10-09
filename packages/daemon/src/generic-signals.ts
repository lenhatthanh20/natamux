import type { TaskService } from './task-service.js'
import { MAIN_TERMINAL } from './terminal-service.js'

// Agents whose hooks Helmsman reads. Their terminal output is not needed for their state.
const AGENTS_WITH_HOOKS: readonly string[] = ['claude-code']

const BELL = '\x07'

// Output comes in many small pieces. Looking at the task for each would read its file each time.
const LOOK_EVERY_MS = 10_000
// Some agents ring the bell as they start (GitHub Copilot does); that is not a question.
export const START_BELL_MS = 15_000

// What any agent in a task's first terminal tells by its output alone:
// - output is a sign of life, so a running task does not look quiet while it prints;
// - the terminal bell, which many command line tools ring when they wait for the user, moves a
//   running task to needs input. Agents with hooks are left to their hooks.
export class GenericSignals {
  private readonly tasks: TaskService
  private readonly now: () => number
  private readonly lastLook = new Map<string, number>()

  constructor(tasks: TaskService, now: () => number = Date.now) {
    this.tasks = tasks
    this.now = now
  }

  onOutput(taskId: string, terminal: string, data: string): void {
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
        payload: { reason: 'terminal bell' },
      })
      return
    }
    this.tasks.recordActivity(taskId)
  }
}
