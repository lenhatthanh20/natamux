import type { TaskEvent } from './events.js'
import {
  type Gate,
  isBranch,
  isGroup,
  type StageKind,
  type Step,
  stageName,
  stagesOf,
  stepIndex,
} from './stage.js'
import type { Task } from './task.js'
import type { TaskState } from './task-state.js'

// `not-taken`: a stage of a branch where the task took another path, or none.
export type StageStatus = 'done' | 'current' | 'upcoming' | 'skipped' | 'not-taken'

// One stage as the progress views show it: where it is, and what proves it.
export type StageProgress = {
  stage: string
  name: string
  kind: StageKind
  gate: Gate
  output: string | null
  // The group the stage runs in, side by side with others.
  group: string | null
  // The path of a branch the stage is on.
  path: string | null
  // Why the task took the path, on the first stage of the path it took.
  because: string | null
  status: StageStatus
  // The state of a current stage.
  state: TaskState | null
  // How many times the stage started; more than one means it was sent back.
  rounds: number
  startedAt: string | null
  endedAt: string | null
  approvedBy: TaskEvent['source'] | null
  // What the last checks found.
  passes: string[]
  reasons: string[]
  // Why the stage was sent back the last time.
  sentBackFor: string | null
  // The last send back came from a CI failure, not from a person or a review.
  sentBackByCi: boolean
}

const ENDS: readonly TaskEvent['type'][] = [
  'stage.approved',
  'review.approved',
  'stage.passed',
  'stage.skipped',
  'stage.checked',
]

const APPROVALS: readonly TaskEvent['type'][] = ['stage.approved', 'review.approved']

// A send back names the stage it goes to in `to`; the daemon fills it in when the sender did not.
function sendsBackTo(event: TaskEvent): string | undefined {
  const sendBack =
    event.type === 'stage.reopened' ||
    event.type === 'review.changes-requested' ||
    event.type === 'agent.resumed'
  return sendBack && typeof event.payload.to === 'string' ? event.payload.to : undefined
}

function strings(value: unknown): string[] {
  return Array.isArray(value)
    ? value.filter((item): item is string => typeof item === 'string')
    : []
}

// The ids of the stages whose end enters step `index`. The end of the step before a skipped
// branch enters the step after it.
function endersOf(
  steps: readonly Step[],
  index: number,
  paths: Task['paths'] | undefined,
): string[] {
  const before = steps[index - 1]
  if (before === undefined) {
    return []
  }
  const ids = stagesOf(before).map((stage) => stage.id)
  return isBranch(before) && paths?.[before.id] === ''
    ? [...ids, ...endersOf(steps, index - 1, paths)]
    : ids
}

// The progress of every stage of a task, in order. `events` are the task's accepted events,
// oldest first.
export function stageProgress(
  task: Pick<Task, 'stage' | 'state' | 'lanes'> & { paths?: Task['paths'] },
  events: readonly TaskEvent[],
  steps: readonly Step[],
): StageProgress[] {
  const current = stepIndex(steps, task.stage)
  // When a stage began: the end of the stage or step before it, or a send back into it. Stages
  // that only wait for a person (review, release) have no start event of their own.
  const enteredAt = (into: readonly string[], from: readonly string[]) =>
    events
      .filter(
        (event) =>
          (ENDS.includes(event.type) && from.includes(String(event.payload.stage))) ||
          into.includes(sendsBackTo(event) ?? ''),
      )
      .at(-1)?.at ?? null
  return steps.flatMap((step, index) =>
    stagesOf(step).map((stage) => {
      // A stage in a path runs like a step of its own, after the stage before it in the path.
      const path = isBranch(step)
        ? step.paths.find((each) => each.stages.includes(stage))
        : undefined
      const place = path?.stages.indexOf(stage) ?? -1
      const previous = path?.stages[place - 1]
      const taken = isBranch(step) ? task.paths?.[step.id] : undefined
      const notTaken = taken !== undefined && taken !== path?.id
      const here = path?.stages.findIndex((each) => each.id === task.stage) ?? -1
      // The event that chose the path the task took.
      const chosen =
        path !== undefined && place === 0 && taken === path.id
          ? events.filter((event) => event.payload.path === path.id).at(-1)
          : undefined
      const entered = enteredAt(
        path === undefined ? stagesOf(step).map((each) => each.id) : [stage.id],
        previous === undefined ? endersOf(steps, index, task.paths) : [previous.id],
      )

      const own = events.filter((event) => event.payload.stage === stage.id)
      const starts = own.filter((event) => event.type === 'stage.started')
      const lastStart = starts.at(-1)
      const sinceStart = lastStart === undefined ? own : own.slice(own.indexOf(lastStart))
      const ended = sinceStart.filter((event) => ENDS.includes(event.type)).at(-1)
      const approved = sinceStart.filter((event) => APPROVALS.includes(event.type)).at(-1)
      const checked = sinceStart.filter((event) => event.type === 'stage.checked').at(-1)
      const failed = sinceStart.filter((event) => event.type === 'stage.check-failed').at(-1)
      // Why the stage's agent failed, such as a shell that could not find it.
      const agentFailed = sinceStart
        .filter((event) => event.type === 'agent.failed' || event.type === 'stage.failed')
        .at(-1)
      const skipped = ended?.type === 'stage.skipped'
      const reopened = events.filter((event) => sendsBackTo(event) === stage.id).at(-1)

      const laneState = isGroup(step) ? task.lanes[stage.id] : task.state
      let status: StageStatus
      let state: TaskState | null = null
      if (notTaken) {
        status = 'not-taken'
      } else if (index < current || task.state === 'done' || (here >= 0 && place < here)) {
        status = skipped ? 'skipped' : 'done'
      } else if (index > current || current < 0 || (path !== undefined && place > here)) {
        status = 'upcoming'
      } else if (laneState === 'done') {
        status = skipped ? 'skipped' : 'done'
      } else {
        status = 'current'
        state = laneState ?? task.state
      }

      return {
        stage: stage.id,
        name: stageName(stage),
        kind: stage.kind,
        gate: stage.gate,
        output: stage.output ?? null,
        group: isGroup(step) ? step.id : null,
        path: path?.id ?? null,
        because: typeof chosen?.payload.because === 'string' ? chosen.payload.because : null,
        status,
        state,
        rounds: status === 'upcoming' || notTaken ? 0 : Math.max(starts.length, 1),
        startedAt: notTaken
          ? null
          : (lastStart?.at ?? (status === 'upcoming' ? null : (chosen?.at ?? entered))),
        endedAt: status === 'done' || status === 'skipped' ? (ended?.at ?? null) : null,
        approvedBy: approved?.source ?? null,
        passes: strings(checked?.payload.passes),
        reasons:
          failed !== undefined && (checked === undefined || failed.at > checked.at)
            ? strings(failed.payload.reasons)
            : state === 'failed' && typeof agentFailed?.payload.reason === 'string'
              ? [agentFailed.payload.reason]
              : [],
        sentBackFor: typeof reopened?.payload.reason === 'string' ? reopened.payload.reason : null,
        sentBackByCi: reopened?.payload.ci === true,
      }
    }),
  )
}
