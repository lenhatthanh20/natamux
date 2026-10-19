import type { EventType, TaskState } from '@natamux/core'
import { shorten } from './summary-text.js'

// What an agent tells Natamux, whatever the agent calls it.
// `turn-paused`: the turn ended, but the agent's background work will wake it again.
export const AGENT_SIGNALS = [
  'prompt',
  'activity',
  'asking',
  'turn-ended',
  'turn-paused',
  'failed',
] as const

export type AgentSignal = (typeof AGENT_SIGNALS)[number]

// A hook of one agent, turned into the common form.
export type NormalizedHook = {
  signal: AgentSignal
  cwd?: string
  details: Record<string, unknown>
  // One line on what the agent does now, such as "Edit src/a.ts". Shown on the dashboard.
  summary?: string
}

// One adapter per agent. `normalize` throws a ZodError for input that is not a hook of this agent,
// and returns undefined for a hook that Natamux does not use.
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
      return state === 'needs-input' || state === 'review' || state === 'failed'
        ? [{ type: 'agent.resumed', payload }]
        : []

    case 'activity':
      // A tool ran, so a permission that was asked for has been answered, or the agent works
      // again after an API error.
      return state === 'needs-input' || state === 'failed'
        ? [{ type: 'agent.resumed', payload }]
        : []

    case 'asking':
      return state === 'running' ? [{ type: 'agent.asked', payload }] : []

    case 'turn-ended':
      return endTurn(state, 'agent.finished', payload)

    case 'turn-paused':
      return endTurn(state, 'agent.waiting', payload)

    case 'failed':
      return endTurn(state, 'agent.failed', payload)
  }
}

// A path inside the session's folder is shown from there.
function relativePath(path: string, cwd: string | undefined): string {
  if (cwd && path.startsWith(`${cwd}/`)) {
    return path.slice(cwd.length + 1)
  }
  return path
}

// A hook's JSON in Claude Code's shape; the other adapters turn theirs into it.
type ToolCall = {
  hook_event_name: string
  tool_name?: string
  tool_input?: Record<string, unknown>
  cwd?: string
}

// "Edit src/a.ts", "Bash: pnpm test", or the tool name for any other tool.
export function toolSummary(input: ToolCall): string | undefined {
  const tool = input.tool_name
  if (!tool) {
    return undefined
  }
  const toolInput = input.tool_input ?? {}
  const text = (key: string) => (typeof toolInput[key] === 'string' ? toolInput[key] : undefined)
  const file = text('file_path') ?? text('notebook_path') ?? text('absolute_path') ?? text('path')
  if (file) {
    return shorten(`${tool} ${relativePath(file, input.cwd)}`)
  }
  const command = text('command')
  if (command) {
    return shorten(`${tool}: ${command}`)
  }
  const pattern = text('pattern') ?? text('url') ?? text('query') ?? text('description')
  return shorten(pattern ? `${tool} ${pattern}` : tool)
}
