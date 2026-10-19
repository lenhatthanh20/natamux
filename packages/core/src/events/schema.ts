import { z } from 'zod'

export const EVENT_SOURCES = ['agent', 'human', 'git', 'ci', 'system'] as const

export const EVENT_TYPES = [
  'task.created',
  'task.readied',
  'task.blocked',
  'task.unblocked',
  'task.retried',
  'task.cancelled',
  'agent.started',
  'agent.asked',
  'agent.resumed',
  'agent.finished',
  'agent.failed',
  'review.approved',
  'review.changes-requested',
  'state.overridden',
  'workspace.created',
  'workspace.removed',
  'agent.active',
  'git.committed',
  'git.pushed',
  'pr.opened',
  'agent.launched',
  'task.edited',
  'pr.merged',
  'pr.closed',
  'ci.passed',
  'ci.failed',
  'review.commented',
  'stage.started',
  'stage.reported',
  'stage.check-failed',
  'stage.checked',
  'stage.approved',
  'stage.passed',
  'stage.failed',
  'stage.reopened',
  'stage.skipped',
  'context.fetched',
  'issue.linked',
  'issue.created',
  'agent.waiting',
  'agent.stopped',
  'attachment.added',
  'stage.workflow-set',
  'stage.ci-set',
  'stage.ci-started',
  'stage.skip-set',
] as const

export const taskEventSchema = z.object({
  taskId: z.string().min(1),
  source: z.enum(EVENT_SOURCES),
  type: z.enum(EVENT_TYPES),
  at: z.iso.datetime(),
  payload: z.record(z.string(), z.unknown()).default({}),
})

export type TaskEvent = z.infer<typeof taskEventSchema>
export type EventType = TaskEvent['type']

// Events that pass a manual gate or set a state by hand. Only a person may send them, never the
// agent token of a task terminal.
export const GATE_EVENT_TYPES: readonly EventType[] = [
  'review.approved',
  'stage.approved',
  'stage.passed',
  'stage.skipped',
  'state.overridden',
]

// Events the daemon records without a change of state.
export const RECORD_ONLY_EVENT_TYPES: readonly EventType[] = [
  'task.created',
  'workspace.created',
  'workspace.removed',
  'agent.active',
  'git.committed',
  'git.pushed',
  'pr.opened',
  'agent.launched',
  'task.edited',
  'pr.closed',
  'ci.passed',
  'ci.failed',
  'review.commented',
  'context.fetched',
  'issue.linked',
  'issue.created',
  'attachment.added',
  'stage.workflow-set',
  'stage.ci-set',
  'stage.ci-started',
  'stage.skip-set',
]
