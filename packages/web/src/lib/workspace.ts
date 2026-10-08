import type { TaskState } from '@helmsman/core'
import type { StoredEvent } from './api'
import type { WorkspaceTab } from './view-store'

type GitButton = {
  label: string
  // Why the button does nothing now. Undefined: it works.
  reason?: string
}

function ofBase(base: string | null): string {
  return base === null ? '' : ` of ${base}`
}

// The Push button. `ahead` is the number of commits ahead of the base branch, null when unknown.
export function pushButton(
  branch: string | null,
  ahead: number | null,
  base: string | null,
): GitButton {
  if (branch === null) {
    return { label: 'Push', reason: 'This task has no branch to push' }
  }
  if (ahead === 0) {
    return { label: 'Push', reason: `Nothing to push: 0 commits ahead${ofBase(base)}` }
  }
  return { label: ahead === null ? 'Push' : `Push ↑${ahead}` }
}

// The button that opens a pull request. The short label is for a narrow main area.
export function pullRequestButton(
  branch: string | null,
  ahead: number | null,
  base: string | null,
  compact: boolean,
): GitButton {
  const label = compact ? 'Open PR' : 'Create pull request'
  if (branch === null) {
    return { label, reason: 'This task has no branch yet' }
  }
  if (ahead === 0) {
    return { label, reason: `Push first: 0 commits ahead${ofBase(base)}` }
  }
  return { label }
}

// What the agent asked, from the newest "agent.asked" event that carries a message.
export function latestQuestion(events: readonly StoredEvent[]): string | null {
  for (const { event } of [...events].reverse()) {
    if (event.type !== 'agent.asked') {
      continue
    }
    const message = event.payload.message
    if (typeof message === 'string' && message.trim() !== '') {
      return message.replace(/\s+/g, ' ').trim()
    }
  }
  return null
}

export function bannerText(question: string | null): string {
  return question === null
    ? 'The agent is waiting for your answer in Shell 1.'
    : `The agent asks: ${question}`
}

// A task whose agent works or waits opens on its terminal. Another task opens on the tab that
// the user used last in it, and a new one opens on its terminal.
export function tabToOpen(state: TaskState, saved: WorkspaceTab | undefined): WorkspaceTab {
  if (state === 'running' || state === 'needs-input') {
    return 'terminal'
  }
  return saved ?? 'terminal'
}
