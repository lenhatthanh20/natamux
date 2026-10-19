import type { StagePosition } from '../stages/position.js'
import { isGroup, type Step } from '../stages/schema.js'
import { stageOfTerminal } from '../stages/terminal-lane.js'
import type { TaskState } from '../tasks/task-state.js'
import type { EventType, TaskEvent } from './schema.js'

// A move of one stage: the states it may happen in, and the state it leads to. `finish` ends the
// stage; `gate` ends it when its gate is auto and waits for approval when it is manual.
type Move = { from: readonly TaskState[]; to: TaskState | 'finish' | 'gate' }

const MOVES: Partial<Record<EventType, Move>> = {
  'task.readied': { from: ['draft'], to: 'ready' },
  'task.blocked': { from: ['ready', 'running'], to: 'blocked' },
  'task.unblocked': { from: ['blocked'], to: 'ready' },
  'task.retried': { from: ['failed'], to: 'ready' },
  'agent.started': { from: ['ready'], to: 'running' },
  'stage.started': { from: ['draft', 'ready'], to: 'running' },
  'agent.asked': { from: ['running'], to: 'needs-input' },
  // A prompt typed to an agent whose stage waits for approval takes the stage up again. So does a
  // prompt after an API error failed the turn: the agent's session goes on.
  'agent.resumed': { from: ['needs-input', 'review', 'failed'], to: 'running' },
  // The agent ended its turn without saying the stage is done (`natamux done`), for example to
  // ask something in plain text. The daemon drops the turn end that follows a report.
  'agent.finished': { from: ['running'], to: 'needs-input' },
  'agent.failed': { from: ['running', 'needs-input'], to: 'failed' },
  // The agent ended its turn, but its background work (a shell, a monitor, a scheduled prompt)
  // will wake it again, so the stage goes on running.
  'agent.waiting': { from: ['running'], to: 'running' },
  // Natamux ended the agent, for example with its workspace. The stage starts again by hand.
  'agent.stopped': { from: ['running', 'needs-input'], to: 'ready' },
  // "Mark as done" from needs-input runs the checks again.
  'stage.reported': { from: ['running', 'needs-input'], to: 'running' },
  'stage.checked': { from: ['running'], to: 'gate' },
  // Approve anyway: a stage whose checks failed twice can still be approved.
  'stage.approved': { from: ['review', 'needs-input'], to: 'finish' },
  'review.approved': { from: ['review', 'needs-input'], to: 'finish' },
  'stage.passed': { from: ['ready', 'running', 'review'], to: 'finish' },
  'stage.failed': { from: ['running'], to: 'failed' },
  'stage.skipped': {
    from: ['draft', 'ready', 'running', 'needs-input', 'review', 'blocked', 'failed'],
    to: 'finish',
  },
}

export function payloadString(event: TaskEvent, key: string): string | undefined {
  const value = event.payload[key]
  return typeof value === 'string' && value !== '' ? value : undefined
}

// The position with the record of the paths taken, when there is one.
export function withPaths(
  position: StagePosition,
  paths: Record<string, string> | undefined,
): StagePosition {
  return paths === undefined ? position : { ...position, paths }
}

// The state of one stage of a step: its lane in a group, else the state of the step.
export function laneStateOf(
  position: StagePosition,
  step: Step,
  stage: string,
): TaskState | undefined {
  return isGroup(step) ? position.lanes[stage] : position.state
}

// The move an event makes in one stage. A failed check keeps the stage running while the agent
// fixes it; the last try waits for you.
export function moveOf(event: TaskEvent): Move | undefined {
  return event.type === 'stage.check-failed'
    ? { from: ['running'], to: event.payload.final === true ? 'needs-input' : 'running' }
    : MOVES[event.type]
}

// The stage an event is about. Outside a group it is the current stage. Inside one: the stage it
// names, else the stage of the terminal it came from, else the only stage the move fits.
export function laneOf(
  steps: readonly Step[],
  step: Step,
  position: StagePosition,
  event: TaskEvent,
  move: Move,
): string | undefined {
  const named = payloadString(event, 'stage')
  if (!isGroup(step) || named !== undefined) {
    return named ?? step.id
  }
  const terminal = payloadString(event, 'terminal')
  if (terminal !== undefined) {
    return stageOfTerminal(steps, step, terminal)?.id
  }
  const fitting = step.parallel.filter((stage) => {
    const state = position.lanes[stage.id]
    return state !== undefined && move.from.includes(state)
  })
  return fitting.length === 1 ? fitting[0]?.id : undefined
}
