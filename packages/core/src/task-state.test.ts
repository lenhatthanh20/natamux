import { describe, expect, it } from 'vitest'
import {
  canTransition,
  InvalidTransitionError,
  isTerminalState,
  needsMe,
  nextStates,
  TASK_STATES,
  type TaskState,
  transition,
} from './task-state.js'

describe('task state machine', () => {
  const allowed: [TaskState, TaskState][] = [
    ['draft', 'ready'],
    ['draft', 'cancelled'],
    ['ready', 'running'],
    ['ready', 'blocked'],
    ['running', 'needs-input'],
    ['running', 'review'],
    ['running', 'failed'],
    ['running', 'blocked'],
    ['needs-input', 'running'],
    ['review', 'done'],
    ['review', 'running'],
    ['blocked', 'ready'],
    ['failed', 'ready'],
  ]

  const rejected: [TaskState, TaskState][] = [
    ['draft', 'running'],
    ['draft', 'done'],
    ['ready', 'review'],
    ['needs-input', 'review'],
    ['review', 'failed'],
    ['done', 'running'],
    ['cancelled', 'ready'],
  ]

  it.each(allowed)('allows %s -> %s', (from, to) => {
    expect(canTransition(from, to)).toBe(true)
  })

  it.each(rejected)('rejects %s -> %s', (from, to) => {
    expect(canTransition(from, to)).toBe(false)
  })

  it('lets every non-terminal state be cancelled', () => {
    for (const state of TASK_STATES) {
      if (!isTerminalState(state)) {
        expect(canTransition(state, 'cancelled')).toBe(true)
      }
    }
  })

  it('has exactly done and cancelled as terminal states', () => {
    expect(TASK_STATES.filter(isTerminalState)).toEqual(['done', 'cancelled'])
  })

  it('never lists a state as its own next state', () => {
    for (const state of TASK_STATES) {
      expect(nextStates(state)).not.toContain(state)
    }
  })

  it('returns the target state on a valid transition', () => {
    expect(transition('ready', 'running')).toBe('running')
  })

  it('throws InvalidTransitionError on an invalid transition', () => {
    expect(() => transition('done', 'running')).toThrow(InvalidTransitionError)
  })

  it('marks only the states that need the user', () => {
    expect(TASK_STATES.filter(needsMe)).toEqual(['needs-input', 'review', 'blocked', 'failed'])
  })
})
