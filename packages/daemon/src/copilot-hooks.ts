import { z } from 'zod'
import type { AgentAdapter, AgentSignal } from './agent-signals.js'
import { toolSummary } from './claude-code-hooks.js'

// The event a hook of GitHub Copilot CLI belongs to. Copilot does not name it in the JSON, so
// `helmsman hook copilot <event>` adds it (see copilotHookFile).
export const COPILOT_EVENT_FIELD = 'helmsman_event'

// The part of the JSON that Copilot CLI sends to a hook which Helmsman reads, in its camelCase form
// (docs.github.com, Copilot CLI hooks reference).
export const copilotInputSchema = z.object({
  [COPILOT_EVENT_FIELD]: z.string().min(1),
  cwd: z.string().optional(),
  sessionId: z.string().optional(),
  toolName: z.string().optional(),
  // A JSON text or an object, depending on the tool.
  toolArgs: z.unknown().optional(),
  recoverable: z.boolean().optional(),
  error: z.unknown().optional(),
})

export type CopilotInput = z.infer<typeof copilotInputSchema>

export function copilotSignal(input: CopilotInput): AgentSignal | undefined {
  switch (input[COPILOT_EVENT_FIELD]) {
    case 'userPromptSubmitted':
      return 'prompt'
    case 'preToolUse':
    case 'postToolUse':
    case 'postToolUseFailure':
      return 'activity'
    case 'permissionRequest':
      return 'asking'
    case 'agentStop':
      return 'turn-ended'
    // An error Copilot gets over (a retried model call) does not end the work.
    case 'errorOccurred':
      return input.recoverable === false ? 'failed' : undefined
    default:
      return undefined
  }
}

function toolArgs(value: unknown): Record<string, unknown> {
  if (typeof value === 'string') {
    try {
      const parsed: unknown = JSON.parse(value)
      return typeof parsed === 'object' && parsed !== null
        ? (parsed as Record<string, unknown>)
        : {}
    } catch {
      return {}
    }
  }
  return typeof value === 'object' && value !== null ? (value as Record<string, unknown>) : {}
}

export const copilotAdapter: AgentAdapter = {
  id: 'copilot',
  normalize(raw) {
    const input = copilotInputSchema.parse(raw)
    const signal = copilotSignal(input)
    if (signal === undefined) {
      return undefined
    }
    const event = input[COPILOT_EVENT_FIELD]
    const summary =
      event === 'preToolUse'
        ? toolSummary({
            hook_event_name: event,
            tool_name: input.toolName,
            tool_input: toolArgs(input.toolArgs),
            cwd: input.cwd,
          })
        : event === 'userPromptSubmitted'
          ? 'Reading the prompt'
          : undefined
    return {
      signal,
      cwd: input.cwd,
      details: {
        hook: event,
        ...(input.sessionId === undefined ? {} : { sessionId: input.sessionId }),
      },
      summary,
    }
  },
}
