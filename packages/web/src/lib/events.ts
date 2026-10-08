import type { EventType } from '@helmsman/core'

// One short sentence for every kind of event. `Record<EventType, …>` makes the compiler ask for a
// sentence whenever a new event type is added.
const EVENT_SENTENCE: Record<EventType, string> = {
  'task.created': 'Task created',
  'task.readied': 'Marked ready',
  'task.blocked': 'Blocked',
  'task.unblocked': 'Unblocked',
  'task.retried': 'Retried',
  'task.cancelled': 'Cancelled',
  'agent.started': 'Agent started',
  'agent.asked': 'Agent is waiting for you',
  'agent.resumed': 'Agent resumed',
  'agent.finished': 'Agent finished its turn',
  'agent.failed': 'Agent failed',
  'review.approved': 'Approved',
  'review.changes-requested': 'Changes requested',
  'state.overridden': 'State set by hand',
  'workspace.created': 'Workspace created',
  'workspace.removed': 'Workspace removed',
  'agent.active': 'Agent is working',
  'git.committed': 'Committed',
  'git.pushed': 'Pushed',
  'pr.opened': 'Pull request opened',
  'agent.launched': 'Agent launched',
}

export function describeEvent(type: EventType): string {
  return EVENT_SENTENCE[type]
}

export function formatClock(isoTime: string): string {
  return new Date(isoTime).toLocaleTimeString([], {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  })
}
