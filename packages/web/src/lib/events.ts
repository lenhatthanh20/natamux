import type { EventType, TaskEvent } from '@natamux/core'
import type { StoredEvent } from './api'

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
  'task.edited': 'Details edited',
  'pr.merged': 'Pull request merged',
  'pr.closed': 'Pull request closed',
  'ci.passed': 'Checks passed',
  'ci.failed': 'Checks failed',
  'review.commented': 'Review comment sent to the agent',
  'stage.started': 'Stage started',
  'stage.reported': 'Stage reported done',
  'stage.check-failed': 'A stage check failed',
  'stage.checked': 'Stage checks passed',
  'stage.approved': 'Stage approved',
  'stage.passed': 'Stage passed',
  'stage.failed': 'Stage failed',
  'stage.reopened': 'Sent back to an earlier stage',
  'stage.skipped': 'Stage skipped',
  'context.fetched': 'Context fetched',
  'issue.linked': 'Issue linked',
  'issue.created': 'Issue created',
  'agent.waiting': 'Agent waits for its background work',
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

const ACTOR: Record<TaskEvent['source'], string> = {
  human: 'You',
  agent: 'Agent',
  git: 'Daemon',
  ci: 'Daemon',
  system: 'Daemon',
}

export function actorOf(source: TaskEvent['source']): string {
  return ACTOR[source]
}

export type ActivityLine = {
  // The first event of the run, which keys the line.
  seq: number
  accepted: boolean
  type: EventType
  actor: string
  // How many events in a row were the same.
  count: number
  // The newest event of the run: its time is the time of the line.
  at: string
  // More about the newest event, such as the background work an agent waits for.
  detail?: string
}

function detailOf(event: TaskEvent): string | undefined {
  const waitingFor = event.payload.waitingFor
  if (event.type !== 'agent.waiting' || !Array.isArray(waitingFor)) {
    return undefined
  }
  const entries = waitingFor.filter((entry): entry is string => typeof entry === 'string')
  return entries.length > 0 ? entries.join('; ') : undefined
}

// Consecutive events of the same kind, from the same actor, become one line with a count, so
// "Details edited" four times in a row reads as "Details edited ×4". Oldest first in, oldest first out.
export function groupEvents(events: readonly StoredEvent[]): ActivityLine[] {
  const lines: ActivityLine[] = []
  for (const { seq, accepted, event } of events) {
    const actor = actorOf(event.source)
    const last = lines[lines.length - 1]
    const detail = detailOf(event)
    if (last?.type === event.type && last.actor === actor && last.accepted === accepted) {
      last.count += 1
      last.at = event.at
      if (detail !== undefined) {
        last.detail = detail
      }
    } else {
      lines.push({
        seq,
        accepted,
        type: event.type,
        actor,
        count: 1,
        at: event.at,
        ...(detail === undefined ? {} : { detail }),
      })
    }
  }
  return lines
}

// "Details edited ×4 · You"
export function describeLine(line: ActivityLine): string {
  const times = line.count > 1 ? ` ×${line.count}` : ''
  const detail = line.detail === undefined ? '' : `: ${line.detail}`
  return `${describeEvent(line.type)}${times}${detail} · ${line.actor}`
}
