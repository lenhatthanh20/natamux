import { dialog } from 'electron'
import type { AppState } from './state.js'

// A hung daemon must not hold Cmd+Q; with no answer, quit goes on without the dialog.
const RUNNING_TASKS_TIMEOUT_MS = 3000

// The number of tasks whose agent works now. Quitting stops them.
async function runningTasks(state: AppState): Promise<number> {
  try {
    const response = await fetch(`${state.dashboardUrl}/tasks?state=running`, {
      headers: { authorization: `Bearer ${state.daemonToken}` },
      signal: AbortSignal.timeout(RUNNING_TASKS_TIMEOUT_MS),
    })
    const body = (await response.json()) as { tasks?: unknown[] }
    return body.tasks?.length ?? 0
  } catch {
    return 0
  }
}

export async function confirmStop(state: AppState, action: string): Promise<boolean> {
  const running = await runningTasks(state)
  if (running === 0) {
    return true
  }
  const answer = await dialog.showMessageBox({
    type: 'warning',
    message: `${running === 1 ? 'An agent is' : `${running} agents are`} still working`,
    detail: `${action} stops the daemon, and with it every agent and shell. Tasks, worktrees and their changes stay, and the agents start again when Natamux does.`,
    buttons: [action, 'Cancel'],
    defaultId: 1,
    cancelId: 1,
  })
  return answer.response === 0
}
