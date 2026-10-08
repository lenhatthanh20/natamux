import type { EventType, TaskState } from '@helmsman/core'

// What an agent tells Helmsman, whatever the agent calls it.
export const AGENT_SIGNALS = ['prompt', 'activity', 'asking', 'turn-ended', 'failed'] as const

export type AgentSignal = (typeof AGENT_SIGNALS)[number]

// A hook of one agent, turned into the common form.
export type NormalizedHook = {
  signal: AgentSignal
  cwd?: string
  details: Record<string, unknown>
}

// One adapter per agent. `normalize` throws a ZodError for input that is not a hook of this agent,
// and returns undefined for a hook that Helmsman does not use.
export type AgentAdapter = {
  id: string
  normalize: (raw: unknown) => NormalizedHook | undefined
}

export type PlannedEvent = {
  type: EventType
  payload: Record<string, unknown>
}

// Brings a task that waits for the user back to running, then adds the closing event.
function endTurn(
  state: TaskState,
  closing: EventType,
  payload: Record<string, unknown>,
): PlannedEvent[] {
  if (state === 'running') {
    return [{ type: closing, payload }]
  }
  if (state === 'needs-input') {
    return [
      { type: 'agent.resumed', payload },
      { type: closing, payload },
    ]
  }
  return []
}

// Decides which task events a signal means for a task in this state. An empty list means "ignore it",
// so the event log does not fill up with calls that change nothing.
export function planSignalEvents(
  state: TaskState,
  signal: AgentSignal,
  payload: Record<string, unknown>,
): PlannedEvent[] {
  switch (signal) {
    case 'prompt':
      if (state === 'draft') {
        return [
          { type: 'task.readied', payload },
          { type: 'agent.started', payload },
        ]
      }
      if (state === 'ready') {
        return [{ type: 'agent.started', payload }]
      }
      return state === 'needs-input' || state === 'review'
        ? [{ type: 'agent.resumed', payload }]
        : []

    case 'activity':
      // A tool ran, so a permission that was asked for has been answered.
      return state === 'needs-input' ? [{ type: 'agent.resumed', payload }] : []

    case 'asking':
      return state === 'running' ? [{ type: 'agent.asked', payload }] : []

    case 'turn-ended':
      return endTurn(state, 'agent.finished', payload)

    case 'failed':
      return endTurn(state, 'agent.failed', payload)
  }
}
