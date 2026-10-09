import type { TaskEvent } from './events.js'
import {
  type Gate,
  isGroup,
  type StageKind,
  type Step,
  stageName,
  stagesOf,
  stepIndex,
} from './stage.js'
import type { Task } from './task.js'
import type { TaskState } from './task-state.js'

export type StageStatus = 'done' | 'current' | 'upcoming' | 'skipped'

// One stage as the progress views show it: where it is, and what proves it.
export type StageProgress = {
  stage: string
  name: string
  kind: StageKind
  gate: Gate
  output: string | null
  // The group the stage runs in, side by side with others.
  group: string | null
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
}

const ENDS: readonly TaskEvent['type'][] = [
  'stage.approved',
  'stage.passed',
  'stage.skipped',
  'stage.checked',
]

function strings(value: unknown): string[] {
  return Array.isArray(value)
    ? value.filter((item): item is string => typeof item === 'string')
    : []
}

// The progress of every stage of a task, in order. `events` are the task's accepted events,
// oldest first.
export function stageProgress(
  task: Pick<Task, 'stage' | 'state' | 'lanes'>,
  events: readonly TaskEvent[],
  steps: readonly Step[],
): StageProgress[] {
  const current = stepIndex(steps, task.stage)
  // When a step began: the end of the step before it, or a send back into it. Stages that only
  // wait for a person (review, release) have no start event of their own.
  const entered = steps.map((step, index) => {
    const before = steps[index - 1]
    const ids = new Set(stagesOf(step).map((stage) => stage.id))
    const previous = new Set(before === undefined ? [] : stagesOf(before).map((stage) => stage.id))
    return (
      events
        .filter(
          (event) =>
            (ENDS.includes(event.type) && previous.has(String(event.payload.stage))) ||
            (event.type === 'stage.reopened' && ids.has(String(event.payload.to))),
        )
        .at(-1)?.at ?? null
    )
  })
  return steps.flatMap((step, index) =>
    stagesOf(step).map((stage) => {
      const own = events.filter((event) => event.payload.stage === stage.id)
      const starts = own.filter((event) => event.type === 'stage.started')
      const lastStart = starts.at(-1)
      const sinceStart = lastStart === undefined ? own : own.slice(own.indexOf(lastStart))
      const ended = sinceStart.filter((event) => ENDS.includes(event.type)).at(-1)
      const approved = sinceStart.filter((event) => event.type === 'stage.approved').at(-1)
      const checked = sinceStart.filter((event) => event.type === 'stage.checked').at(-1)
      const failed = sinceStart.filter((event) => event.type === 'stage.check-failed').at(-1)
      const skipped = ended?.type === 'stage.skipped'
      const reopened = events
        .filter((event) => event.type === 'stage.reopened' && event.payload.to === stage.id)
        .at(-1)

      const laneState = isGroup(step) ? task.lanes[stage.id] : task.state
      let status: StageStatus
      let state: TaskState | null = null
      if (index < current || task.state === 'done') {
        status = skipped ? 'skipped' : 'done'
      } else if (index > current || current < 0) {
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
        status,
        state,
        rounds: status === 'upcoming' ? 0 : Math.max(starts.length, 1),
        startedAt: lastStart?.at ?? (status === 'upcoming' ? null : (entered[index] ?? null)),
        endedAt: status === 'done' || status === 'skipped' ? (ended?.at ?? null) : null,
        approvedBy: approved?.source ?? null,
        passes: strings(checked?.payload.passes),
        reasons:
          failed !== undefined && (checked === undefined || failed.at > checked.at)
            ? strings(failed.payload.reasons)
            : [],
        sentBackFor: typeof reopened?.payload.reason === 'string' ? reopened.payload.reason : null,
      }
    }),
  )
}
