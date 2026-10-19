import { DEFAULT_PIPELINE } from '../../stages/default-pipeline.js'
import type { StagePosition } from '../../stages/position.js'
import type { Stage, StageBranch, Step } from '../../stages/schema.js'
import { applyTaskEvent } from '../apply.js'
import type { EventType, TaskEvent } from '../schema.js'

// The pipeline these cases were written for: a person reviews, then the tests run. The default
// pipeline has an agent review after the tests instead.
export const PIPELINE: Step[] = DEFAULT_PIPELINE.flatMap((step): Step[] => {
  if (step.id === 'agent-review') {
    return []
  }
  return step.id === 'testing' ? [{ id: 'review', kind: 'review', gate: 'manual' }, step] : [step]
})

export function event(type: EventType, payload: Record<string, unknown> = {}): TaskEvent {
  return { taskId: 'T-1', source: 'system', type, at: '2026-10-08T09:00:00Z', payload }
}

export function at(stage: string, state: StagePosition['state'], lanes = {}): StagePosition {
  return { stage, state, lanes }
}

export function run(position: StagePosition, ...events: TaskEvent[]) {
  let current = position
  const accepted: boolean[] = []
  for (const next of events) {
    const applied = applyTaskEvent(current, next, PIPELINE)
    current = applied.position
    accepted.push(applied.accepted)
  }
  return { position: current, accepted }
}

export const plan: Stage = { id: 'plan', kind: 'agent', gate: 'manual', output: 'plan.md' }
export const branch: StageBranch = {
  id: 'after-plan',
  paths: [
    {
      id: 'small',
      when: { type: 'output', file: 'plan.md', section: 'Size', contains: 'small' },
      stages: [
        { id: 'implement', kind: 'agent', gate: 'auto' },
        { id: 'testing', kind: 'testing', gate: 'auto' },
      ],
    },
    {
      id: 'pr-review',
      when: { type: 'command', run: 'test -f .no-review' },
      not: true,
      stages: [{ id: 'review', kind: 'review', gate: 'manual' }],
    },
  ],
}
export const steps: Step[] = [plan, branch, { id: 'release', kind: 'release', gate: 'manual' }]
export const SMALL = { 'after-plan': 'small' }

export function on(stage: string, state: StagePosition['state'], paths: Record<string, string>) {
  return { stage, state, lanes: {}, paths }
}

export function walk(position: StagePosition, ...events: TaskEvent[]) {
  let current = position
  const accepted: boolean[] = []
  for (const next of events) {
    const applied = applyTaskEvent(current, next, steps)
    current = applied.position
    accepted.push(applied.accepted)
  }
  return { position: current, accepted }
}
