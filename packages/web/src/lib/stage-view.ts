import {
  type Stage,
  type StageProgress,
  type Step,
  stagesOf,
  type Task,
  type TaskEvent,
} from '@natamux/core'

// "<1m", "4m", "1h 5m", "2d 3h": the time between two moments.
export function formatDuration(from: string, to: Date | string): string {
  const end = typeof to === 'string' ? Date.parse(to) : to.getTime()
  const span = Math.max(0, end - Date.parse(from))
  if (span < 60_000) {
    return '<1m'
  }
  const minutes = Math.round(span / 60_000)
  if (minutes < 60) {
    return `${minutes}m`
  }
  const hours = Math.floor(minutes / 60)
  if (hours < 24) {
    return minutes % 60 === 0 ? `${hours}h` : `${hours}h ${minutes % 60}m`
  }
  const days = Math.floor(hours / 24)
  return hours % 24 === 0 ? `${days}d` : `${days}d ${hours % 24}h`
}

// The time a stage took, or has taken so far.
export function stageTime(progress: StageProgress, now: Date): string | null {
  if (progress.startedAt === null) {
    return null
  }
  if (progress.status === 'current') {
    return `${formatDuration(progress.startedAt, now)} so far`
  }
  return progress.endedAt === null ? null : formatDuration(progress.startedAt, progress.endedAt)
}

// The time of a step: from the first start to the last end of its stages (side by side stages
// overlap). Null until one has started, or while one has no end.
export function stepTime(
  entries: readonly (StageProgress | undefined)[],
  now: Date,
): string | null {
  const started = entries.filter(
    (entry): entry is StageProgress & { startedAt: string } =>
      entry !== undefined && entry.startedAt !== null,
  )
  const first = started
    .map((entry) => entry.startedAt)
    .sort((a, b) => Date.parse(a) - Date.parse(b))[0]
  if (first === undefined) {
    return null
  }
  if (started.some((entry) => entry.status === 'current')) {
    return `${formatDuration(first, now)} so far`
  }
  const ends = started.map((entry) => entry.endedAt)
  if (ends.some((end) => end === null)) {
    return null
  }
  const last = ends
    .filter((end): end is string => end !== null)
    .sort((a, b) => Date.parse(b) - Date.parse(a))[0]
  return last === undefined ? null : formatDuration(first, last)
}

export function findStage(steps: readonly Step[], id: string): Stage | undefined {
  return steps.flatMap(stagesOf).find((stage) => stage.id === id)
}

// What finishes a stage, in one sentence, for a stage not reached yet.
export function finishedWhen(stage: Stage): string {
  if (stage.doneWhen !== undefined) {
    return stage.doneWhen
  }
  const gate = stage.gate === 'manual' ? ', and you approve it' : ''
  switch (stage.kind) {
    case 'context':
      return `Finished when ${stage.output ?? 'context.md'} is written${gate}.`
    case 'review':
      return 'Finished when you approve the changes.'
    case 'release':
      return 'Finished when the pull request is merged.'
    case 'manual':
      return 'Finished when you approve it.'
    case 'testing':
      return stage.command === undefined
        ? 'Finished when the tests are written and pass.'
        : `Finished when ${stage.command} passes.`
    case 'agent': {
      const parts = [
        ...(stage.output === undefined ? [] : [`${stage.output} is written`]),
        ...(stage.sections === undefined || stage.sections.length === 0
          ? []
          : [`it has ${stage.sections.join(', ')}`]),
        ...(stage.requireCommit ? ['the work is committed'] : []),
        ...(stage.check === undefined ? [] : [`${stage.check} passes`]),
      ]
      return parts.length === 0
        ? `Finished when the agent says it is done${gate}.`
        : `Finished when ${parts.join(', ')}${gate}.`
    }
  }
}

// How many stages are finished, of all.
export function stageCount(progress: readonly StageProgress[]): { done: number; total: number } {
  return {
    done: progress.filter((stage) => stage.status === 'done' || stage.status === 'skipped').length,
    total: progress.length,
  }
}

// How many times the task was sent back.
export function sentBackCount(events: readonly TaskEvent[]): number {
  return events.filter(
    (event) => event.type === 'stage.reopened' || event.type === 'review.changes-requested',
  ).length
}

export function stepsKey(task: Pick<Task, 'stages'>): string {
  return JSON.stringify(task.stages)
}
